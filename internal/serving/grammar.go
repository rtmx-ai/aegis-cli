package serving

import "strings"

// ObjectGrammar returns a GBNF grammar (the llama-server `grammar` param) that
// constrains model output to a JSON object with exactly the given string-valued
// keys, in order — so a weak local model returns deterministically parseable
// structured output (THINK-006). Built with no model and verifiable offline; a
// genuine local advantage (grammar-constrained decoding is free, vs. a second
// pass to validate). Returns "" (no constraint) for an empty key set.
func ObjectGrammar(keys []string) string {
	if len(keys) == 0 {
		return ""
	}
	parts := make([]string, len(keys))
	for i, k := range keys {
		parts[i] = gbnfLit(`"`+k+`"`) + ` ws ":" ws string ws`
	}
	root := `root ::= "{" ws ` + strings.Join(parts, `"," ws `) + `"}"`
	return root + "\n" +
		`string ::= "\"" ( [^"\\] | "\\" . )* "\""` + "\n" +
		`ws ::= [ \t\n]*` + "\n"
}

// gbnfLit returns a GBNF double-quoted literal that matches s verbatim.
func gbnfLit(s string) string {
	esc := strings.ReplaceAll(s, `\`, `\\`)
	esc = strings.ReplaceAll(esc, `"`, `\"`)
	return `"` + esc + `"`
}
