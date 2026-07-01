package index

import (
	"crypto/sha256"
	"encoding/hex"
	"os"
	"path/filepath"
	"sort"
)

// Snapshot maps a repo-relative path to its content hash — a Merkle leaf per file
// (INDEX-004). Comparing two snapshots yields exactly the files that changed, so
// re-indexing touches only those, not the whole tree.
type Snapshot map[string]string

// SnapshotDir hashes root's source files into a Snapshot (INDEX-004), reusing the
// same file set the repo map indexes.
func SnapshotDir(root string) Snapshot {
	snap := Snapshot{}
	files, _ := goFiles(root)
	for _, rel := range files {
		data, err := os.ReadFile(filepath.Join(root, rel))
		if err != nil {
			continue
		}
		h := sha256.Sum256(data)
		snap[rel] = hex.EncodeToString(h[:])
	}
	return snap
}

// ChangedSince returns the paths added or modified vs prev (changed) and the paths
// deleted (removed), so incremental re-indexing processes only what changed
// (INDEX-004). The tree-sitter edit() re-parse + file watcher are the CGO/OS
// follow-on; this is the model-free change-detection core.
func ChangedSince(prev, cur Snapshot) (changed, removed []string) {
	for p, h := range cur {
		if prev[p] != h {
			changed = append(changed, p)
		}
	}
	for p := range prev {
		if _, ok := cur[p]; !ok {
			removed = append(removed, p)
		}
	}
	sort.Strings(changed)
	sort.Strings(removed)
	return changed, removed
}
