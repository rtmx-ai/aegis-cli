//! Embedding-based codebase retrieval for context injection.
//!
//! Provides file chunking, embedding provider abstraction, vector index
//! with cosine similarity search, context injection, and cache invalidation.
//!
//! REQ-AGENT-043: Parent requirement for embedding retrieval.
//! REQ-AGENT-045: File chunker with overlap for code files.
//! REQ-AGENT-046: Embedding provider abstraction (local or API).
//! REQ-AGENT-047: Vector index with cosine similarity search.
//! REQ-AGENT-048: Context injection: top-k chunks into system prompt.
//! REQ-AGENT-049: Cache invalidation on file change.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

// ---------------------------------------------------------------------------
// REQ-AGENT-045: File chunker with overlap
// ---------------------------------------------------------------------------

/// A chunk of source code with line range metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct Chunk {
    /// File path this chunk came from.
    pub file_path: PathBuf,
    /// Starting line number (1-based).
    pub start_line: usize,
    /// Ending line number (1-based, inclusive).
    pub end_line: usize,
    /// The chunk text content.
    pub content: String,
}

/// Split a file's content into chunks of `chunk_size` lines with `overlap`
/// lines of overlap between consecutive chunks.
///
/// Preserves function boundaries when possible: if a chunk boundary falls
/// inside a function, extend to the next blank line or closing brace.
pub fn chunk_file(
    file_path: &Path,
    content: &str,
    chunk_size: usize,
    overlap: usize,
) -> Vec<Chunk> {
    if content.is_empty() || chunk_size == 0 {
        return Vec::new();
    }

    let lines: Vec<&str> = content.lines().collect();
    let total = lines.len();
    if total == 0 {
        return Vec::new();
    }

    let mut chunks = Vec::new();
    let step = if chunk_size > overlap {
        chunk_size - overlap
    } else {
        1
    };

    let mut start = 0;
    while start < total {
        let mut end = (start + chunk_size).min(total);

        // Try to extend to a natural boundary (blank line or closing brace)
        // within a small window past the chunk end.
        if end < total {
            let boundary_window = overlap.clamp(3, 10);
            let search_end = (end + boundary_window).min(total);
            for (offset, line) in lines[end..search_end].iter().enumerate() {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed == "}" {
                    end = end + offset + 1;
                    break;
                }
            }
        }

        let chunk_content: String = lines[start..end].join("\n");
        chunks.push(Chunk {
            file_path: file_path.to_path_buf(),
            start_line: start + 1,
            end_line: end,
            content: chunk_content,
        });

        start += step;
        if start >= total {
            break;
        }
    }

    chunks
}

// ---------------------------------------------------------------------------
// REQ-AGENT-046: Embedding provider abstraction
// ---------------------------------------------------------------------------

/// A fixed-size embedding vector.
pub type EmbeddingVec = Vec<f32>;

/// Trait for embedding providers (local or API-based).
#[async_trait::async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Embed a single text string into a vector.
    async fn embed(&self, text: &str) -> Result<EmbeddingVec, String>;

    /// Embed a batch of texts. Default implementation calls embed() in
    /// sequence; providers may override for batch API efficiency.
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<EmbeddingVec>, String> {
        let mut results = Vec::with_capacity(texts.len());
        for text in texts {
            results.push(self.embed(text).await?);
        }
        Ok(results)
    }

    /// The dimensionality of the embedding vectors produced.
    fn dimensions(&self) -> usize;
}

/// A simple hash-based mock embedding provider for testing.
/// Produces deterministic embeddings by hashing the input text.
pub struct HashEmbeddingProvider {
    dims: usize,
}

impl HashEmbeddingProvider {
    pub fn new(dims: usize) -> Self {
        Self { dims }
    }
}

#[async_trait::async_trait]
impl EmbeddingProvider for HashEmbeddingProvider {
    async fn embed(&self, text: &str) -> Result<EmbeddingVec, String> {
        // Deterministic pseudo-embedding from text hash.
        let mut vec = vec![0.0f32; self.dims];
        for (i, byte) in text.bytes().enumerate() {
            let idx = i % self.dims;
            vec[idx] += (byte as f32) / 255.0;
        }
        // Normalize to unit vector.
        let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-10);
        for v in &mut vec {
            *v /= norm;
        }
        Ok(vec)
    }

    fn dimensions(&self) -> usize {
        self.dims
    }
}

// ---------------------------------------------------------------------------
// REQ-AGENT-052: Ollama Embedding Provider
// ---------------------------------------------------------------------------

/// Configuration for the Ollama embedding provider.
#[derive(Debug, Clone)]
pub struct OllamaEmbeddingConfig {
    /// Base URL for Ollama API (default: http://localhost:11434).
    pub base_url: String,
    /// Model name (default: nomic-embed-text).
    pub model: String,
    /// Expected embedding dimensions for the chosen model.
    pub dimensions: usize,
}

impl Default for OllamaEmbeddingConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:11434".to_string(),
            model: "nomic-embed-text".to_string(),
            dimensions: 768,
        }
    }
}

/// Embedding provider that calls the Ollama `/api/embed` endpoint.
pub struct OllamaEmbeddingProvider {
    config: OllamaEmbeddingConfig,
    client: reqwest::Client,
}

impl OllamaEmbeddingProvider {
    /// Create a new Ollama embedding provider with the given config.
    pub fn new(config: OllamaEmbeddingConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }

    /// Create with a custom reqwest client (for testing with mock servers).
    pub fn with_client(config: OllamaEmbeddingConfig, client: reqwest::Client) -> Self {
        Self { config, client }
    }
}

#[async_trait::async_trait]
impl EmbeddingProvider for OllamaEmbeddingProvider {
    async fn embed(&self, text: &str) -> Result<EmbeddingVec, String> {
        let url = format!("{}/api/embed", self.config.base_url);

        let body = serde_json::json!({
            "model": self.config.model,
            "input": text,
        });

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Ollama request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Ollama returned {status}: {body}"));
        }

        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse Ollama response: {e}"))?;

        // Ollama /api/embed returns { "embeddings": [[...]] }
        let embeddings = json
            .get("embeddings")
            .and_then(|v| v.as_array())
            .ok_or("Missing 'embeddings' field in response")?;

        let first = embeddings
            .first()
            .and_then(|v| v.as_array())
            .ok_or("Empty embeddings array")?;

        let vec: EmbeddingVec = first
            .iter()
            .filter_map(|v| v.as_f64().map(|f| f as f32))
            .collect();

        if vec.len() != self.config.dimensions {
            return Err(format!(
                "Expected {} dimensions, got {}",
                self.config.dimensions,
                vec.len()
            ));
        }

        Ok(vec)
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<EmbeddingVec>, String> {
        let url = format!("{}/api/embed", self.config.base_url);

        let body = serde_json::json!({
            "model": self.config.model,
            "input": texts,
        });

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Ollama batch request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Ollama returned {status}: {body}"));
        }

        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse Ollama batch response: {e}"))?;

        let embeddings = json
            .get("embeddings")
            .and_then(|v| v.as_array())
            .ok_or("Missing 'embeddings' field in batch response")?;

        let mut results = Vec::with_capacity(texts.len());
        for emb in embeddings {
            let arr = emb.as_array().ok_or("Non-array element in embeddings")?;
            let vec: EmbeddingVec = arr
                .iter()
                .filter_map(|v| v.as_f64().map(|f| f as f32))
                .collect();
            if vec.len() != self.config.dimensions {
                return Err(format!(
                    "Expected {} dimensions, got {}",
                    self.config.dimensions,
                    vec.len()
                ));
            }
            results.push(vec);
        }

        Ok(results)
    }

    fn dimensions(&self) -> usize {
        self.config.dimensions
    }
}

// ---------------------------------------------------------------------------
// REQ-AGENT-047: Vector index with cosine similarity search
// ---------------------------------------------------------------------------

/// An indexed chunk with its embedding vector.
#[derive(Debug, Clone)]
struct IndexedChunk {
    chunk: Chunk,
    embedding: EmbeddingVec,
}

/// In-memory vector index for cosine similarity search over code chunks.
pub struct VectorIndex {
    entries: Vec<IndexedChunk>,
}

impl VectorIndex {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Add a chunk with its precomputed embedding to the index.
    pub fn insert(&mut self, chunk: Chunk, embedding: EmbeddingVec) {
        self.entries.push(IndexedChunk { chunk, embedding });
    }

    /// Search for the top-k most similar chunks to the query embedding.
    pub fn search(&self, query_embedding: &EmbeddingVec, top_k: usize) -> Vec<(f32, &Chunk)> {
        let mut scored: Vec<(f32, &Chunk)> = self
            .entries
            .iter()
            .map(|entry| {
                let sim = cosine_similarity(query_embedding, &entry.embedding);
                (sim, &entry.chunk)
            })
            .collect();

        // Sort by similarity descending.
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(top_k);
        scored
    }

    /// Number of indexed chunks.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the index is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Remove all entries for a given file path (used for cache
    /// invalidation on file change).
    pub fn remove_file(&mut self, file_path: &Path) {
        self.entries
            .retain(|entry| entry.chunk.file_path != file_path);
    }
}

impl Default for VectorIndex {
    fn default() -> Self {
        Self::new()
    }
}

/// Cosine similarity between two vectors.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a < 1e-10 || norm_b < 1e-10 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

// ---------------------------------------------------------------------------
// REQ-AGENT-048: Context injection: top-k chunks into system prompt
// ---------------------------------------------------------------------------

/// Format retrieved chunks into a context string for system prompt injection.
pub fn format_context_injection(results: &[(f32, &Chunk)]) -> String {
    if results.is_empty() {
        return String::new();
    }

    let mut context = String::from(
        "\n<retrieved_context>\n\
         The following code snippets are relevant to the current task:\n\n",
    );

    for (i, (score, chunk)) in results.iter().enumerate() {
        context.push_str(&format!(
            "--- {}:{}-{} (relevance: {:.2}) ---\n{}\n\n",
            chunk.file_path.display(),
            chunk.start_line,
            chunk.end_line,
            score,
            chunk.content,
        ));
        if i >= 9 {
            // Cap at 10 chunks in the prompt.
            break;
        }
    }

    context.push_str("</retrieved_context>\n");
    context
}

// ---------------------------------------------------------------------------
// REQ-AGENT-049: Cache invalidation on file change
// ---------------------------------------------------------------------------

/// Tracks file modification times for cache invalidation.
pub struct FileChangeTracker {
    /// Map from file path to last known modification time.
    mod_times: HashMap<PathBuf, SystemTime>,
}

impl FileChangeTracker {
    pub fn new() -> Self {
        Self {
            mod_times: HashMap::new(),
        }
    }

    /// Record the current modification time for a file.
    pub fn record(&mut self, path: &Path, mod_time: SystemTime) {
        self.mod_times.insert(path.to_path_buf(), mod_time);
    }

    /// Check if a file has changed since last recording.
    /// Returns true if the file is new or has a different mod time.
    pub fn has_changed(&self, path: &Path, current_mod_time: SystemTime) -> bool {
        match self.mod_times.get(path) {
            Some(recorded) => *recorded != current_mod_time,
            None => true, // new file, not yet tracked
        }
    }

    /// Get all files that have changed given current modification times.
    /// Returns paths that need re-indexing.
    pub fn changed_files(&self, current_times: &[(PathBuf, SystemTime)]) -> Vec<PathBuf> {
        current_times
            .iter()
            .filter(|(path, time)| self.has_changed(path, *time))
            .map(|(path, _)| path.clone())
            .collect()
    }

    /// Update recorded times after re-indexing.
    pub fn update_batch(&mut self, times: &[(PathBuf, SystemTime)]) {
        for (path, time) in times {
            self.record(path, *time);
        }
    }
}

impl Default for FileChangeTracker {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // rtmx:req REQ-AGENT-045
    #[test]
    fn test_file_chunker_overlap() {
        let content = (1..=20)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");

        let chunks = chunk_file(Path::new("test.rs"), &content, 5, 2);

        // Should produce multiple chunks.
        assert!(
            chunks.len() >= 3,
            "expected >= 3 chunks, got {}",
            chunks.len()
        );

        // First chunk starts at line 1.
        assert_eq!(chunks[0].start_line, 1);

        // Chunks should overlap.
        if chunks.len() >= 2 {
            assert!(
                chunks[1].start_line <= chunks[0].end_line,
                "chunks must overlap: chunk1 starts at {}, chunk0 ends at {}",
                chunks[1].start_line,
                chunks[0].end_line,
            );
        }

        // Each chunk should have content.
        for chunk in &chunks {
            assert!(!chunk.content.is_empty());
            assert!(chunk.start_line <= chunk.end_line);
        }
    }

    // rtmx:req REQ-AGENT-045
    #[test]
    fn test_file_chunker_empty_input() {
        let chunks = chunk_file(Path::new("empty.rs"), "", 5, 2);
        assert!(chunks.is_empty());
    }

    // rtmx:req REQ-AGENT-045
    #[test]
    fn test_file_chunker_small_file() {
        let content = "line 1\nline 2\nline 3";
        let chunks = chunk_file(Path::new("small.rs"), content, 10, 2);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].start_line, 1);
        assert_eq!(chunks[0].end_line, 3);
    }

    // rtmx:req REQ-AGENT-045
    #[test]
    fn test_file_chunker_preserves_file_path() {
        let content = "fn main() {}\nfn helper() {}";
        let path = Path::new("src/main.rs");
        let chunks = chunk_file(path, content, 10, 0);
        assert_eq!(chunks[0].file_path, path);
    }

    // rtmx:req REQ-AGENT-046
    #[tokio::test]
    async fn test_hash_embedding_provider_produces_vectors() {
        let provider = HashEmbeddingProvider::new(8);
        let embedding = provider.embed("hello world").await.unwrap();
        assert_eq!(embedding.len(), 8);

        // Should be normalized (unit vector).
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!(
            (norm - 1.0).abs() < 0.01,
            "embedding should be unit vector, norm = {}",
            norm,
        );
    }

    // rtmx:req REQ-AGENT-046
    #[tokio::test]
    async fn test_hash_embedding_deterministic() {
        let provider = HashEmbeddingProvider::new(8);
        let e1 = provider.embed("same text").await.unwrap();
        let e2 = provider.embed("same text").await.unwrap();
        assert_eq!(e1, e2, "same input must produce same embedding");
    }

    // rtmx:req REQ-AGENT-046
    #[tokio::test]
    async fn test_hash_embedding_batch() {
        let provider = HashEmbeddingProvider::new(8);
        let texts = vec!["hello".to_string(), "world".to_string()];
        let results = provider.embed_batch(&texts).await.unwrap();
        assert_eq!(results.len(), 2);
        assert_ne!(results[0], results[1]);
    }

    // rtmx:req REQ-AGENT-047
    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 0.001);
    }

    // rtmx:req REQ-AGENT-047
    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 0.001);
    }

    // rtmx:req REQ-AGENT-047
    #[tokio::test]
    async fn test_vector_index_search_returns_top_k() {
        let provider = HashEmbeddingProvider::new(8);
        let mut index = VectorIndex::new();

        // Index some chunks.
        let chunks = vec![
            Chunk {
                file_path: PathBuf::from("a.rs"),
                start_line: 1,
                end_line: 5,
                content: "fn auth_login() { validate_token() }".into(),
            },
            Chunk {
                file_path: PathBuf::from("b.rs"),
                start_line: 1,
                end_line: 5,
                content: "fn render_chart() { draw_bars() }".into(),
            },
            Chunk {
                file_path: PathBuf::from("c.rs"),
                start_line: 1,
                end_line: 5,
                content: "fn auth_verify() { check_jwt() }".into(),
            },
        ];

        for chunk in chunks {
            let emb = provider.embed(&chunk.content).await.unwrap();
            index.insert(chunk, emb);
        }

        assert_eq!(index.len(), 3);

        // Search for auth-related content.
        let query_emb = provider.embed("authentication login token").await.unwrap();
        let results = index.search(&query_emb, 2);

        assert_eq!(results.len(), 2);
        // Results should be sorted by similarity descending.
        assert!(results[0].0 >= results[1].0);
    }

    // rtmx:req REQ-AGENT-047
    #[test]
    fn test_vector_index_remove_file() {
        let mut index = VectorIndex::new();
        let chunk = Chunk {
            file_path: PathBuf::from("remove_me.rs"),
            start_line: 1,
            end_line: 5,
            content: "fn test() {}".into(),
        };
        index.insert(chunk, vec![1.0, 0.0, 0.0]);
        assert_eq!(index.len(), 1);

        index.remove_file(Path::new("remove_me.rs"));
        assert!(index.is_empty());
    }

    // rtmx:req REQ-AGENT-048
    #[test]
    fn test_context_injection_top_k() {
        let chunks = [
            Chunk {
                file_path: PathBuf::from("src/auth.rs"),
                start_line: 10,
                end_line: 20,
                content: "fn verify_token() { /* ... */ }".into(),
            },
            Chunk {
                file_path: PathBuf::from("src/main.rs"),
                start_line: 1,
                end_line: 5,
                content: "fn main() {}".into(),
            },
        ];
        let results = [(0.95, &chunks[0]), (0.72, &chunks[1])];

        let context = format_context_injection(&results);

        assert!(context.contains("<retrieved_context>"));
        assert!(context.contains("</retrieved_context>"));
        assert!(context.contains("src/auth.rs:10-20"));
        assert!(context.contains("relevance: 0.95"));
        assert!(context.contains("fn verify_token()"));
    }

    // rtmx:req REQ-AGENT-048
    #[test]
    fn test_context_injection_empty() {
        let results: Vec<(f32, &Chunk)> = vec![];
        let context = format_context_injection(&results);
        assert!(context.is_empty());
    }

    // rtmx:req REQ-AGENT-049
    #[test]
    fn test_file_change_tracker_detects_new_file() {
        let tracker = FileChangeTracker::new();
        let now = SystemTime::now();
        assert!(
            tracker.has_changed(Path::new("new.rs"), now),
            "untracked file should be considered changed"
        );
    }

    // rtmx:req REQ-AGENT-049
    #[test]
    fn test_file_change_tracker_detects_modification() {
        let mut tracker = FileChangeTracker::new();
        let path = Path::new("test.rs");
        let t1 = SystemTime::now();
        tracker.record(path, t1);

        // Same time: no change.
        assert!(!tracker.has_changed(path, t1));

        // Different time: changed.
        let t2 = t1 + std::time::Duration::from_secs(1);
        assert!(tracker.has_changed(path, t2));
    }

    // rtmx:req REQ-AGENT-049
    #[test]
    fn test_file_change_tracker_batch_update() {
        let mut tracker = FileChangeTracker::new();
        let now = SystemTime::now();
        let times = vec![(PathBuf::from("a.rs"), now), (PathBuf::from("b.rs"), now)];

        tracker.update_batch(&times);

        assert!(!tracker.has_changed(Path::new("a.rs"), now));
        assert!(!tracker.has_changed(Path::new("b.rs"), now));
    }

    // rtmx:req REQ-AGENT-049
    #[test]
    fn test_file_change_tracker_changed_files() {
        let mut tracker = FileChangeTracker::new();
        let t1 = SystemTime::now();
        let t2 = t1 + std::time::Duration::from_secs(1);

        tracker.record(Path::new("unchanged.rs"), t1);
        tracker.record(Path::new("changed.rs"), t1);

        let current = vec![
            (PathBuf::from("unchanged.rs"), t1), // same
            (PathBuf::from("changed.rs"), t2),   // modified
            (PathBuf::from("new.rs"), t1),       // new
        ];

        let changed = tracker.changed_files(&current);
        assert_eq!(changed.len(), 2);
        assert!(changed.contains(&PathBuf::from("changed.rs")));
        assert!(changed.contains(&PathBuf::from("new.rs")));
    }

    // rtmx:req REQ-AGENT-043
    #[tokio::test]
    async fn test_embedding_retrieval_returns_relevant_chunks() {
        // End-to-end test: chunk files, embed, index, query, inject.
        let provider = HashEmbeddingProvider::new(16);
        let mut index = VectorIndex::new();

        // Index a "codebase" of two files.
        let auth_code = "fn verify_jwt(token: &str) -> bool {\n    \
                          let claims = decode(token);\n    \
                          claims.exp > now()\n}";
        let chart_code = "fn draw_bar_chart(data: &[f64]) {\n    \
                           for val in data {\n        \
                           draw_rect(val)\n    }\n}";

        for chunk in chunk_file(Path::new("auth.rs"), auth_code, 10, 2) {
            let emb = provider.embed(&chunk.content).await.unwrap();
            index.insert(chunk, emb);
        }
        for chunk in chunk_file(Path::new("chart.rs"), chart_code, 10, 2) {
            let emb = provider.embed(&chunk.content).await.unwrap();
            index.insert(chunk, emb);
        }

        // Query for auth-related content.
        let query_emb = provider
            .embed("JWT token verification authentication")
            .await
            .unwrap();
        let results = index.search(&query_emb, 2);

        assert!(!results.is_empty());

        // Format as context injection.
        let context = format_context_injection(&results);
        assert!(context.contains("<retrieved_context>"));
        assert!(!context.is_empty());
    }

    // --- REQ-AGENT-052: Ollama Embedding Provider ---

    // rtmx:req REQ-AGENT-052
    #[tokio::test]
    async fn test_ollama_embed_single() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        // Simulate Ollama /api/embed response with 4-dimensional vector.
        let response_body = serde_json::json!({
            "embeddings": [[0.1, 0.2, 0.3, 0.4]]
        });

        Mock::given(method("POST"))
            .and(path("/api/embed"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .mount(&mock_server)
            .await;

        let config = OllamaEmbeddingConfig {
            base_url: mock_server.uri(),
            model: "nomic-embed-text".to_string(),
            dimensions: 4,
        };
        let provider = OllamaEmbeddingProvider::new(config);

        let result = provider.embed("hello world").await.unwrap();
        assert_eq!(result.len(), 4);
        assert!((result[0] - 0.1).abs() < 1e-6);
        assert!((result[3] - 0.4).abs() < 1e-6);
    }

    // rtmx:req REQ-AGENT-052
    #[tokio::test]
    async fn test_ollama_embed_batch() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        let response_body = serde_json::json!({
            "embeddings": [
                [0.1, 0.2, 0.3],
                [0.4, 0.5, 0.6],
            ]
        });

        Mock::given(method("POST"))
            .and(path("/api/embed"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .mount(&mock_server)
            .await;

        let config = OllamaEmbeddingConfig {
            base_url: mock_server.uri(),
            model: "test-model".to_string(),
            dimensions: 3,
        };
        let provider = OllamaEmbeddingProvider::new(config);

        let texts = vec!["hello".to_string(), "world".to_string()];
        let results = provider.embed_batch(&texts).await.unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].len(), 3);
        assert_eq!(results[1].len(), 3);
        assert!((results[1][0] - 0.4).abs() < 1e-6);
    }

    // rtmx:req REQ-AGENT-052
    #[tokio::test]
    async fn test_ollama_embed_dimension_mismatch_returns_error() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        // Server returns 3 dims but config expects 4.
        let response_body = serde_json::json!({
            "embeddings": [[0.1, 0.2, 0.3]]
        });

        Mock::given(method("POST"))
            .and(path("/api/embed"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .mount(&mock_server)
            .await;

        let config = OllamaEmbeddingConfig {
            base_url: mock_server.uri(),
            model: "test-model".to_string(),
            dimensions: 4,
        };
        let provider = OllamaEmbeddingProvider::new(config);

        let result = provider.embed("test").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Expected 4 dimensions, got 3"));
    }

    // rtmx:req REQ-AGENT-052
    #[tokio::test]
    async fn test_ollama_embed_server_error() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/embed"))
            .respond_with(ResponseTemplate::new(500).set_body_string("internal error"))
            .mount(&mock_server)
            .await;

        let config = OllamaEmbeddingConfig {
            base_url: mock_server.uri(),
            model: "test-model".to_string(),
            dimensions: 4,
        };
        let provider = OllamaEmbeddingProvider::new(config);

        let result = provider.embed("test").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("500"));
    }

    // rtmx:req REQ-AGENT-052
    #[tokio::test]
    async fn test_ollama_embed_connection_refused() {
        let config = OllamaEmbeddingConfig {
            base_url: "http://127.0.0.1:1".to_string(), // nothing listening
            model: "test-model".to_string(),
            dimensions: 4,
        };
        let provider = OllamaEmbeddingProvider::new(config);

        let result = provider.embed("test").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("request failed"));
    }

    // rtmx:req REQ-AGENT-052
    #[tokio::test]
    async fn test_ollama_config_default_values() {
        let config = OllamaEmbeddingConfig::default();
        assert_eq!(config.base_url, "http://localhost:11434");
        assert_eq!(config.model, "nomic-embed-text");
        assert_eq!(config.dimensions, 768);
    }
}
