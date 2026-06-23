package rtmx

import (
	"encoding/csv"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
)

// Store is a read/write view over an rtmx CSV-in-git database — rtmx's
// source-of-truth data model. It backs the real Client implementations:
// requirement detail, the next-claimable scan, status writeback, and claim
// coordination (the .rtmx/claims/ directory) all operate here, since the MCP
// server exposes no status-writeback tool and `next` returns only ids.
type Store struct {
	dbPath    string
	claimsDir string
}

// NewStore opens the database at dbPath (e.g. ".rtmx/database.csv"). Claims live
// in a sibling claims/ directory.
func NewStore(dbPath string) *Store {
	return &Store{dbPath: dbPath, claimsDir: filepath.Join(filepath.Dir(dbPath), "claims")}
}

// statusFromCSV maps an rtmx CSV status to a Client lifecycle Status.
func statusFromCSV(s string) Status {
	switch strings.ToUpper(strings.TrimSpace(s)) {
	case "COMPLETE":
		return StatusClosed
	case "BLOCKED":
		return StatusBlocked
	case "PROPOSED":
		return StatusProposed
	default: // OPEN, PLANNED, PARTIAL, "" → claimable/open
		return StatusOpen
	}
}

// statusToCSV maps a lifecycle Status back to an rtmx CSV status.
func statusToCSV(s Status) string {
	switch s {
	case StatusClosed:
		return "COMPLETE"
	case StatusBlocked:
		return "BLOCKED"
	case StatusProposed:
		return "PROPOSED"
	default:
		return "OPEN"
	}
}

// rawDB is the parsed CSV with its header, for round-trip-preserving writes.
type rawDB struct {
	header []string
	rows   [][]string
	col    map[string]int
}

func (s *Store) read() (*rawDB, error) {
	f, err := os.Open(s.dbPath)
	if err != nil {
		return nil, fmt.Errorf("rtmx store: open %s: %w", s.dbPath, err)
	}
	defer f.Close()
	recs, err := csv.NewReader(f).ReadAll()
	if err != nil {
		return nil, fmt.Errorf("rtmx store: parse %s: %w", s.dbPath, err)
	}
	if len(recs) == 0 {
		return nil, fmt.Errorf("rtmx store: %s is empty", s.dbPath)
	}
	col := map[string]int{}
	for i, h := range recs[0] {
		col[h] = i
	}
	for _, need := range []string{"req_id", "requirement_text", "status", "dependencies"} {
		if _, ok := col[need]; !ok {
			return nil, fmt.Errorf("rtmx store: missing column %q", need)
		}
	}
	return &rawDB{header: recs[0], rows: recs[1:], col: col}, nil
}

func (db *rawDB) get(row []string, name string) string {
	if i, ok := db.col[name]; ok && i < len(row) {
		return row[i]
	}
	return ""
}

// toRequirement builds a Requirement from a raw CSV row.
func (db *rawDB) toRequirement(row []string) *Requirement {
	id := db.get(row, "req_id")
	prefix := db.get(row, "category")
	if prefix == "" {
		if parts := strings.Split(id, "-"); len(parts) >= 2 {
			prefix = parts[1]
		}
	}
	var tests []string
	if tm, tf := db.get(row, "test_module"), db.get(row, "test_function"); tf != "" {
		tests = []string{tm + "::" + tf}
	}
	var deps []string
	for _, d := range strings.Split(db.get(row, "dependencies"), "|") {
		if d = strings.TrimSpace(d); d != "" {
			deps = append(deps, d)
		}
	}
	return &Requirement{
		ID:       id,
		Prefix:   prefix,
		Title:    db.get(row, "requirement_text"),
		Status:   statusFromCSV(db.get(row, "status")),
		Tests:    tests,
		Deps:     deps,
		SpecFile: db.get(row, "requirement_file"),
		Notes:    db.get(row, "notes"),
	}
}

// Requirements returns every requirement in the database, in file order.
func (s *Store) Requirements() ([]*Requirement, error) {
	db, err := s.read()
	if err != nil {
		return nil, err
	}
	out := make([]*Requirement, 0, len(db.rows))
	for _, row := range db.rows {
		out = append(out, db.toRequirement(row))
	}
	return out, nil
}

// ByID returns the requirement with the given id, or an error if absent.
func (s *Store) ByID(id string) (*Requirement, error) {
	reqs, err := s.Requirements()
	if err != nil {
		return nil, err
	}
	for _, r := range reqs {
		if r.ID == id {
			return r, nil
		}
	}
	return nil, fmt.Errorf("rtmx store: unknown requirement %s", id)
}

// Next returns the first claimable requirement: open status, all dependencies
// closed, and not currently claimed. It returns nil when the backlog is drained.
func (s *Store) Next() (*Requirement, error) {
	reqs, err := s.Requirements()
	if err != nil {
		return nil, err
	}
	status := map[string]Status{}
	for _, r := range reqs {
		status[r.ID] = r.Status
	}
	for _, r := range reqs {
		if r.Status != StatusOpen {
			continue
		}
		if s.isClaimed(r.ID) {
			continue
		}
		ready := true
		for _, d := range r.Deps {
			if status[d] != StatusClosed {
				ready = false
				break
			}
		}
		if ready {
			return r, nil
		}
	}
	return nil, nil
}

// SetStatus rewrites the requirement's status in the CSV, preserving all other
// columns and field order.
func (s *Store) SetStatus(id string, st Status) error {
	db, err := s.read()
	if err != nil {
		return err
	}
	si := db.col["status"]
	found := false
	for _, row := range db.rows {
		if db.get(row, "req_id") == id {
			if si < len(row) {
				row[si] = statusToCSV(st)
				found = true
			}
			break
		}
	}
	if !found {
		return fmt.Errorf("rtmx store: unknown requirement %s", id)
	}
	return s.write(db)
}

func (s *Store) write(db *rawDB) error {
	tmp, err := os.CreateTemp(filepath.Dir(s.dbPath), "db-*.csv")
	if err != nil {
		return err
	}
	w := csv.NewWriter(tmp)
	_ = w.Write(db.header)
	_ = w.WriteAll(db.rows)
	w.Flush()
	if err := w.Error(); err != nil {
		tmp.Close()
		os.Remove(tmp.Name())
		return err
	}
	if err := tmp.Close(); err != nil {
		os.Remove(tmp.Name())
		return err
	}
	return os.Rename(tmp.Name(), s.dbPath) // atomic replace
}

func (s *Store) claimPath(id string) string {
	return filepath.Join(s.claimsDir, id+".json")
}

func (s *Store) isClaimed(id string) bool {
	_, err := os.Stat(s.claimPath(id))
	return err == nil
}

// Claim records an exclusive claim; a second claim of the same id fails.
func (s *Store) Claim(id, agent string) error {
	if err := os.MkdirAll(s.claimsDir, 0o755); err != nil {
		return err
	}
	f, err := os.OpenFile(s.claimPath(id), os.O_CREATE|os.O_EXCL|os.O_WRONLY, 0o644)
	if err != nil {
		return fmt.Errorf("rtmx store: %s already claimed: %w", id, err)
	}
	defer f.Close()
	fmt.Fprintf(f, `{"req_id":%q,"agent_id":%q}`+"\n", id, agent)
	return nil
}

// Release drops a claim (idempotent).
func (s *Store) Release(id string) error {
	err := os.Remove(s.claimPath(id))
	if err != nil && !os.IsNotExist(err) {
		return err
	}
	return nil
}

// claimedIDs lists currently-claimed ids (sorted; used in tests/diagnostics).
func (s *Store) claimedIDs() []string {
	entries, err := os.ReadDir(s.claimsDir)
	if err != nil {
		return nil
	}
	var ids []string
	for _, e := range entries {
		ids = append(ids, strings.TrimSuffix(e.Name(), ".json"))
	}
	sort.Strings(ids)
	return ids
}
