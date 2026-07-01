package index

import (
	"strings"
	"testing"
)

// TestGrepFirstDoctrine → REQ-INDEX-003: grep/glob/LSP/repo-map are sanctioned
// retrieval; embeddings/vector/RAG engines are rejected (air-gap + small model).
func TestGrepFirstDoctrine(t *testing.T) {
	for _, ok := range []string{"grep", "glob", "LSP", "repomap", "repo-map", "ripgrep", "scip"} {
		if err := GuardRetrieval(ok); err != nil {
			t.Errorf("%q must be sanctioned grep-first retrieval: %v", ok, err)
		}
	}
	for _, bad := range []string{"embeddings", "vector", "rag", "chroma", "faiss", "sqlite-vec", "openai-embeddings"} {
		if err := GuardRetrieval(bad); err == nil {
			t.Errorf("%q (an embeddings engine) must be rejected", bad)
		} else if !strings.Contains(err.Error(), "INDEX-003") {
			t.Errorf("rejection should cite the doctrine: %v", err)
		}
	}
}
