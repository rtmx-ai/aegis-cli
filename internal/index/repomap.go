// Package index builds an air-gapped, model-free repo map: a ranked, token-
// budgeted skeleton of a codebase (definition signatures) so a small local model
// can call real symbols without loading whole files (INDEX-001, Aider's repo-map
// approach). Go is parsed with go/ast; other first-class languages via the pure-Go
// ctags-style extractor (INDEX-009/010) — no CGO, no network. Ranking is
// personalized PageRank over a def/ref file graph (AST edges for Go, text edges for
// the rest), seeded by the task's mentioned identifiers.
package index

import (
	"go/ast"
	"go/parser"
	"go/printer"
	"go/token"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
)

// Symbol is a renderable top-level definition.
type Symbol struct {
	Name string
	Sig  string // one-line elided signature
}

// Options configures Build.
type Options struct {
	Root        string   // repo root to scan
	Mentions    []string // task identifiers/paths that personalize ranking (boost relevant files)
	TokenBudget int      // approximate character budget for the rendered map (default 4000)
}

// Build scans Root's Go sources and returns a ranked, budget-bounded repo map.
func Build(opts Options) (string, error) {
	if opts.TokenBudget <= 0 {
		opts.TokenBudget = 4000
	}
	fset := token.NewFileSet()
	srcFiles, err := sourceFiles(opts.Root)
	if err != nil {
		return "", err
	}
	rels := make([]string, 0, len(srcFiles))
	for rel := range srcFiles {
		rels = append(rels, rel)
	}
	sort.Strings(rels) // deterministic "first defines the name" for edges

	asts := map[string]*ast.File{}
	otherContent := map[string]string{} // INDEX-010: non-Go content, for text edges
	defsByFile := map[string][]Symbol{} // renderable (exported + methods)
	defFileOf := map[string]string{}    // every top-level name -> defining file (for edges)
	for _, rel := range rels {
		abs := filepath.Join(opts.Root, rel)
		if srcFiles[rel] == "go" {
			f, perr := parser.ParseFile(fset, abs, nil, parser.SkipObjectResolution)
			if perr != nil {
				continue // skip unparseable files; never fail the whole map
			}
			asts[rel] = f
			for _, decl := range f.Decls {
				for _, s := range symbolsOf(decl, fset) {
					if _, ok := defFileOf[s.name]; !ok {
						defFileOf[s.name] = rel
					}
					if s.render {
						defsByFile[rel] = append(defsByFile[rel], Symbol{Name: s.name, Sig: s.sig})
					}
				}
			}
			continue
		}
		// INDEX-010: non-Go files via the pure-Go ctags extractor (INDEX-009).
		data, rerr := os.ReadFile(abs)
		if rerr != nil {
			continue
		}
		syms := ExtractSymbols(srcFiles[rel], string(data))
		if len(syms) == 0 {
			continue
		}
		otherContent[rel] = string(data)
		for _, s := range syms {
			if _, ok := defFileOf[s.Name]; !ok {
				defFileOf[s.Name] = rel
			}
			defsByFile[rel] = append(defsByFile[rel], s)
		}
	}

	edges := buildEdges(asts, defFileOf)
	addTextEdges(edges, otherContent, defFileOf) // INDEX-010: language-agnostic def/ref edges
	ranked := make([]string, 0, len(defsByFile))
	for f := range defsByFile {
		ranked = append(ranked, f)
	}
	rank := pageRank(keys(defFileOf, defsByFile), edges, personalize(defsByFile, defFileOf, opts.Mentions), 20, 0.85)
	sort.SliceStable(ranked, func(i, j int) bool {
		if rank[ranked[i]] != rank[ranked[j]] {
			return rank[ranked[i]] > rank[ranked[j]]
		}
		return ranked[i] < ranked[j]
	})
	return render(ranked, defsByFile, opts.TokenBudget), nil
}

type rawSym struct {
	name, sig string
	render    bool // include in the rendered map (exported funcs, methods, types)
}

func symbolsOf(decl ast.Decl, fset *token.FileSet) []rawSym {
	switch d := decl.(type) {
	case *ast.FuncDecl:
		method := d.Recv != nil && len(d.Recv.List) > 0
		body := d.Body
		d.Body = nil
		var sb strings.Builder
		_ = printer.Fprint(&sb, fset, d)
		d.Body = body
		return []rawSym{{name: d.Name.Name, sig: oneLine(sb.String()), render: method || ast.IsExported(d.Name.Name)}}
	case *ast.GenDecl:
		var out []rawSym
		for _, spec := range d.Specs {
			switch s := spec.(type) {
			case *ast.TypeSpec:
				out = append(out, rawSym{name: s.Name.Name, sig: typeSig(s.Name.Name, s.Type, fset), render: ast.IsExported(s.Name.Name)})
			case *ast.ValueSpec:
				kw := "var"
				if d.Tok == token.CONST {
					kw = "const"
				}
				for _, n := range s.Names {
					out = append(out, rawSym{name: n.Name, sig: kw + " " + n.Name, render: ast.IsExported(n.Name)})
				}
			}
		}
		return out
	}
	return nil
}

func typeSig(name string, t ast.Expr, fset *token.FileSet) string {
	switch t.(type) {
	case *ast.StructType:
		return "type " + name + " struct"
	case *ast.InterfaceType:
		return "type " + name + " interface"
	default:
		var sb strings.Builder
		_ = printer.Fprint(&sb, fset, t)
		return "type " + name + " " + oneLine(sb.String())
	}
}

// buildEdges links file F -> file G with weight = number of identifiers in F that
// resolve to a top-level definition in G (F depends on G).
func buildEdges(asts map[string]*ast.File, defFileOf map[string]string) map[string]map[string]float64 {
	edges := map[string]map[string]float64{}
	for file, f := range asts {
		ast.Inspect(f, func(n ast.Node) bool {
			id, ok := n.(*ast.Ident)
			if !ok {
				return true
			}
			def, ok := defFileOf[id.Name]
			if !ok || def == file {
				return true
			}
			if edges[file] == nil {
				edges[file] = map[string]float64{}
			}
			edges[file][def]++
			return true
		})
	}
	return edges
}

func personalize(defsByFile map[string][]Symbol, defFileOf map[string]string, mentions []string) map[string]float64 {
	p := map[string]float64{}
	if len(mentions) == 0 {
		return p
	}
	set := map[string]bool{}
	for _, m := range mentions {
		set[m] = true
		if f, ok := defFileOf[m]; ok {
			p[f] += 1 // a file that defines a mentioned symbol
		}
	}
	for file := range defsByFile {
		for _, m := range mentions {
			if strings.Contains(file, m) {
				p[file] += 2 // the file path itself was mentioned
			}
		}
	}
	return p
}

// pageRank runs personalized PageRank over the file graph.
func pageRank(nodes []string, edges map[string]map[string]float64, personal map[string]float64, iters int, damping float64) map[string]float64 {
	n := len(nodes)
	if n == 0 {
		return map[string]float64{}
	}
	var psum float64
	for _, id := range nodes {
		psum += personal[id]
	}
	pv := map[string]float64{}
	for _, id := range nodes {
		if psum > 0 {
			pv[id] = personal[id] / psum
		} else {
			pv[id] = 1.0 / float64(n)
		}
	}
	out := map[string]float64{}
	for src, dsts := range edges {
		for _, w := range dsts {
			out[src] += w
		}
	}
	rank := map[string]float64{}
	for _, id := range nodes {
		rank[id] = 1.0 / float64(n)
	}
	for it := 0; it < iters; it++ {
		next := map[string]float64{}
		var dangling float64
		for _, id := range nodes {
			next[id] = (1 - damping) * pv[id]
			if out[id] == 0 {
				dangling += rank[id]
			}
		}
		for src, dsts := range edges {
			if out[src] == 0 {
				continue
			}
			for dst, w := range dsts {
				next[dst] += damping * rank[src] * (w / out[src])
			}
		}
		for _, id := range nodes {
			next[id] += damping * dangling * pv[id]
		}
		rank = next
	}
	return rank
}

func render(ranked []string, defsByFile map[string][]Symbol, budget int) string {
	var b strings.Builder
	for _, f := range ranked {
		defs := defsByFile[f]
		if len(defs) == 0 {
			continue
		}
		header := f + ":\n"
		if b.Len()+len(header) > budget {
			break
		}
		b.WriteString(header)
		for _, d := range defs {
			line := "  " + d.Sig + "\n"
			if b.Len()+len(line) > budget {
				return b.String()
			}
			b.WriteString(line)
		}
	}
	return b.String()
}

// sourceFiles returns recognized source files (rel -> canonical language id) under
// root, skipping vendor/.git/testdata/node_modules and Go test files (INDEX-010).
func sourceFiles(root string) (map[string]string, error) {
	out := map[string]string{}
	err := filepath.WalkDir(root, func(path string, d os.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if d.IsDir() {
			b := d.Name()
			if b == "vendor" || b == "testdata" || b == "node_modules" || (strings.HasPrefix(b, ".") && b != ".") {
				return filepath.SkipDir
			}
			return nil
		}
		lang := LangFromPath(path)
		if lang == "" {
			return nil
		}
		if lang == "go" && strings.HasSuffix(path, "_test.go") {
			return nil
		}
		rel, _ := filepath.Rel(root, path)
		out[rel] = lang
		return nil
	})
	return out, err
}

var identRe = regexp.MustCompile(`[A-Za-z_][A-Za-z0-9_]*`)

// addTextEdges links a non-Go file F -> file G for each identifier in F that names a
// top-level definition in another file G — the language-agnostic def/ref signal that
// lets non-Go files rank alongside Go ones under PageRank (INDEX-010).
func addTextEdges(edges map[string]map[string]float64, contentByFile, defFileOf map[string]string) {
	for file, content := range contentByFile {
		for _, w := range identRe.FindAllString(content, -1) {
			def, ok := defFileOf[w]
			if !ok || def == file {
				continue
			}
			if edges[file] == nil {
				edges[file] = map[string]float64{}
			}
			edges[file][def]++
		}
	}
}

// goFiles returns non-test .go paths (relative to root), skipping vendor/.git/testdata.
func goFiles(root string) ([]string, error) {
	var files []string
	err := filepath.WalkDir(root, func(path string, d os.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if d.IsDir() {
			b := d.Name()
			if b == "vendor" || b == "testdata" || (strings.HasPrefix(b, ".") && b != ".") {
				return filepath.SkipDir
			}
			return nil
		}
		if strings.HasSuffix(path, ".go") && !strings.HasSuffix(path, "_test.go") {
			rel, _ := filepath.Rel(root, path)
			files = append(files, rel)
		}
		return nil
	})
	sort.Strings(files)
	return files, err
}

func keys(defFileOf map[string]string, defsByFile map[string][]Symbol) []string {
	set := map[string]bool{}
	for _, f := range defFileOf {
		set[f] = true
	}
	for f := range defsByFile {
		set[f] = true
	}
	out := make([]string, 0, len(set))
	for f := range set {
		out = append(out, f)
	}
	sort.Strings(out)
	return out
}

func oneLine(s string) string {
	return strings.Join(strings.Fields(s), " ")
}
