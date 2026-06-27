// Package origin governs model provenance: a per-country allow/deny policy and a gate
// that fails when the selected model's country of origin is not permitted (MODEL-006/007).
// This is supply-chain / compliance governance (docs/model-compliance.md) — complementary
// to the GUARD egress controls, not a replacement. Origin is recorded per model in the
// catalog (MODEL-005); the policy is an operator-controlled, version-controllable file.
package origin

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
)

const (
	// Allow and Deny are the two policy dispositions.
	Allow = "allow"
	Deny  = "deny"
	// DefaultPolicyPath is the committed policy file; AEGIS_ORIGIN_POLICY overrides it.
	DefaultPolicyPath = "deploy/models/origin-policy.json"
	policyEnv         = "AEGIS_ORIGIN_POLICY"
)

// Policy is a per-country allow/deny disposition with a default for unlisted/unknown
// origins. Allowing a denied origin is an explicit edit to this file — there is no env
// bypass — so the decision stays auditable.
type Policy struct {
	Default   string            `json:"default"`
	Countries map[string]string `json:"countries"`
}

// PolicyPath returns the policy file path: AEGIS_ORIGIN_POLICY if set, else the default.
func PolicyPath() string {
	if p := os.Getenv(policyEnv); p != "" {
		return p
	}
	return DefaultPolicyPath
}

// LoadPolicy reads + validates the origin policy from path, or from PolicyPath() when path
// is empty.
func LoadPolicy(path string) (*Policy, error) {
	if path == "" {
		path = PolicyPath()
	}
	b, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("origin: read policy %s: %w", path, err)
	}
	var p Policy
	if err := json.Unmarshal(b, &p); err != nil {
		return nil, fmt.Errorf("origin: parse policy %s: %w", path, err)
	}
	if err := p.validate(); err != nil {
		return nil, err
	}
	return &p, nil
}

func (p *Policy) validate() error {
	if !disposition(p.Default) {
		return fmt.Errorf("origin: policy default must be %q or %q, got %q", Allow, Deny, p.Default)
	}
	for c, d := range p.Countries {
		if !disposition(d) {
			return fmt.Errorf("origin: policy for %q must be %q or %q, got %q", c, Allow, Deny, d)
		}
	}
	return nil
}

func disposition(s string) bool { return s == Allow || s == Deny }

// Allows reports whether a country (ISO-3166 alpha-2) is permitted: a listed entry wins,
// otherwise the policy default. An empty/unknown country falls to the default (deny-safe
// when the default is deny).
func (p *Policy) Allows(country string) bool {
	if d, ok := p.Countries[strings.ToUpper(strings.TrimSpace(country))]; ok {
		return d == Allow
	}
	return p.Default == Allow
}

// OriginForModel returns the ISO origin recorded for a model file/name in a catalog JSON
// document, matched by the catalog `file` (basename) then `name`. known=false when the
// model is absent or carries no origin.
func OriginForModel(modelName string, catalogJSON []byte) (country string, known bool) {
	base := filepath.Base(modelName)
	var cat struct {
		Models []struct {
			Name   string `json:"name"`
			File   string `json:"file"`
			Origin string `json:"origin"`
		} `json:"models"`
	}
	if json.Unmarshal(catalogJSON, &cat) != nil {
		return "", false
	}
	for _, m := range cat.Models {
		if (m.File == base || m.Name == modelName) && m.Origin != "" {
			return m.Origin, true
		}
	}
	return "", false
}

// Denial is the typed result of a denied origin check.
type Denial struct {
	Model   string
	Country string // "" when the origin is unknown
	Known   bool
}

func (d *Denial) Error() string {
	if !d.Known {
		return fmt.Sprintf("origin: model %q has unknown/unclassified origin — denied by policy (classify it in the catalog, or set the policy default to allow)", d.Model)
	}
	return fmt.Sprintf("origin: model %q origin %q is denied by the origin policy (set %q to allow in %s to permit it)", d.Model, d.Country, d.Country, DefaultPolicyPath)
}

// CheckModel resolves modelName's origin from the catalog and returns a *Denial when the
// policy does not allow it. An unknown origin is denied unless the policy default is allow.
func CheckModel(modelName string, catalogJSON []byte, p *Policy) error {
	country, known := OriginForModel(modelName, catalogJSON)
	if !known {
		if p.Default == Allow {
			return nil
		}
		return &Denial{Model: modelName, Known: false}
	}
	if !p.Allows(country) {
		return &Denial{Model: modelName, Country: country, Known: true}
	}
	return nil
}
