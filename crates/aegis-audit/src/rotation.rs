//! Zstd compression for rotated audit log segments (REQ-AUDIT-009).
//!
//! Compresses inactive (non-current-day) JSONL log files with zstd to
//! reduce disk usage while preserving audit data integrity.

use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

/// Compress a JSONL log file with zstd.
///
/// Input: `~/.aegis/logs/aegis-2026-04-10.jsonl`
/// Output: `~/.aegis/logs/aegis-2026-04-10.jsonl.zst`
///
/// The original `.jsonl` file is removed after successful compression.
pub fn compress_segment(path: &Path) -> io::Result<PathBuf> {
    if !path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("segment not found: {}", path.display()),
        ));
    }

    if is_compressed(path) {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("segment already compressed: {}", path.display()),
        ));
    }

    let content = std::fs::read(path)?;
    let out_path = path.with_extension("jsonl.zst");

    let mut encoder = zstd::Encoder::new(
        std::fs::File::create(&out_path)?,
        3, // default compression level
    )?;
    encoder.write_all(&content)?;
    encoder.finish()?;

    // Remove original only after successful compression.
    std::fs::remove_file(path)?;

    Ok(out_path)
}

/// Decompress a zstd-compressed segment for reading.
///
/// Returns the full decompressed JSONL content as a string.
pub fn decompress_segment(path: &Path) -> io::Result<String> {
    if !path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("compressed segment not found: {}", path.display()),
        ));
    }

    let file = std::fs::File::open(path)?;
    let mut decoder = zstd::Decoder::new(file)?;
    let mut content = String::new();
    decoder.read_to_string(&mut content)?;

    Ok(content)
}

/// Check if a segment is already compressed (has `.zst` extension).
pub fn is_compressed(path: &Path) -> bool {
    path.extension().is_some_and(|ext| ext == "zst")
}

/// Compress all rotated segments (not the current active one).
///
/// Returns the number of segments compressed.
pub fn compress_rotated_segments(log_dir: &Path) -> io::Result<usize> {
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let mut compressed = 0;

    let entries = std::fs::read_dir(log_dir)?;
    for entry in entries.flatten() {
        let path = entry.path();

        // Skip non-jsonl files and already-compressed files.
        if path.extension().is_none_or(|ext| ext != "jsonl") {
            continue;
        }

        // Skip the current day's log file(s).
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.contains(&today) {
            continue;
        }

        // Skip non-aegis files.
        if !name.starts_with("aegis-") {
            continue;
        }

        compress_segment(&path)?;
        compressed += 1;
    }

    Ok(compressed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_sample_segment(dir: &Path, name: &str, content: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, content).unwrap();
        path
    }

    // rtmx:req REQ-AUDIT-009
    #[test]
    fn compress_creates_zst_file() {
        let tmp = TempDir::new().unwrap();
        let path = write_sample_segment(
            tmp.path(),
            "aegis-2026-04-10.jsonl",
            "{\"event\":\"test\"}\n",
        );

        let zst = compress_segment(&path).unwrap();

        assert!(zst.exists(), ".zst file should exist");
        assert!(!path.exists(), "original .jsonl should be removed");
        assert_eq!(
            zst.file_name().unwrap().to_str().unwrap(),
            "aegis-2026-04-10.jsonl.zst"
        );
    }

    // rtmx:req REQ-AUDIT-009
    #[test]
    fn decompress_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let original = "{\"event\":\"test\",\"ts\":1}\n{\"event\":\"test\",\"ts\":2}\n";
        let path = write_sample_segment(tmp.path(), "aegis-2026-04-10.jsonl", original);

        let zst = compress_segment(&path).unwrap();
        let decompressed = decompress_segment(&zst).unwrap();

        assert_eq!(decompressed, original);
    }

    // rtmx:req REQ-AUDIT-009
    #[test]
    fn is_compressed_detects_zst() {
        assert!(is_compressed(Path::new("aegis-2026-04-10.jsonl.zst")));
        assert!(!is_compressed(Path::new("aegis-2026-04-10.jsonl")));
        assert!(!is_compressed(Path::new("aegis-2026-04-10.jsonl.lock")));
    }

    // rtmx:req REQ-AUDIT-009
    #[test]
    fn compress_rotated_skips_active() {
        let tmp = TempDir::new().unwrap();
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

        // Today's file (active -- should NOT be compressed).
        write_sample_segment(
            tmp.path(),
            &format!("aegis-{today}.jsonl"),
            "{\"event\":\"active\"}\n",
        );

        // Old file (should be compressed).
        write_sample_segment(
            tmp.path(),
            "aegis-2025-01-01.jsonl",
            "{\"event\":\"old\"}\n",
        );

        let count = compress_rotated_segments(tmp.path()).unwrap();

        assert_eq!(count, 1, "should compress only the old segment");
        assert!(
            tmp.path().join(format!("aegis-{today}.jsonl")).exists(),
            "today's file should still be uncompressed"
        );
        assert!(
            tmp.path().join("aegis-2025-01-01.jsonl.zst").exists(),
            "old file should be compressed"
        );
    }

    // rtmx:req REQ-AUDIT-009
    #[test]
    fn compress_already_compressed_is_noop() {
        let tmp = TempDir::new().unwrap();
        let path = write_sample_segment(
            tmp.path(),
            "aegis-2026-04-10.jsonl",
            "{\"event\":\"test\"}\n",
        );

        // First compression succeeds.
        let zst = compress_segment(&path).unwrap();

        // Trying to compress the .zst file should error.
        let result = compress_segment(&zst);
        assert!(result.is_err());
    }
}
