package index

import (
	"fmt"
	"strings"
)

// RetrievalDoctrine documents aegis's air-gap retrieval doctrine (INDEX-003): the
// default and only sanctioned retrieval is grep/glob/LSP/repo-map — never an
// embeddings/vector engine, which needs a hosted or heavy local model and adds
// latency a small CPU-bound model can't afford. A Jan-2026 benchmark also shows
// deterministic AST-derived retrieval is ~20x cheaper and more complete.
const RetrievalDoctrine = "grep-first: grep/glob/LSP/repo-map; never embeddings"

// sanctionedRetrieval is the allowed retrieval methods.
var sanctionedRetrieval = map[string]bool{
	"grep": true, "glob": true, "lsp": true, "repomap": true, "repo-map": true, "ripgrep": true, "rg": true, "scip": true,
}

// GuardRetrieval returns an error if method is not a sanctioned grep-first retrieval
// (i.e. it is an embeddings/vector/RAG engine), enforcing the air-gap doctrine
// (INDEX-003).
func GuardRetrieval(method string) error {
	m := strings.ToLower(strings.TrimSpace(method))
	if sanctionedRetrieval[m] {
		return nil
	}
	return fmt.Errorf("index: retrieval method %q is not grep-first (INDEX-003): use grep/glob/LSP/repo-map, not an embeddings engine", method)
}
