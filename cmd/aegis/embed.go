package main

import (
	_ "embed"
	"path/filepath"

	"github.com/rtmx-ai/aegis-cli/internal/origin"
)

//go:embed deploydata/catalog.json
var embeddedCatalog []byte

//go:embed deploydata/origin-policy.json
var embeddedOriginPolicy []byte

//go:embed deploydata/MODEL_REF
var embeddedModelRef []byte

// embeddedDeploy serves the deploy/models data when no on-disk copy is found (OC-033), so an installed
// aegis (Homebrew/.deb) run from any directory can still provision/profile/gate.
var embeddedDeploy = map[string][]byte{
	filepath.ToSlash(filepath.Join("deploy", "models", "catalog.json")):       embeddedCatalog,
	filepath.ToSlash(filepath.Join("deploy", "models", "origin-policy.json")): embeddedOriginPolicy,
	filepath.ToSlash(filepath.Join("deploy", "models", "MODEL_REF")):          embeddedModelRef,
}

// aegisOriginPolicy loads the origin policy from the resolved file, or the embedded default (OC-033).
func aegisOriginPolicy() (*origin.Policy, error) {
	if p, err := origin.LoadPolicy(originPolicyPath()); err == nil {
		return p, nil
	}
	return origin.ParsePolicy(embeddedOriginPolicy)
}

var _ = embeddedModelRef // referenced via embeddedDeploy
