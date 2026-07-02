package serving

import (
	"context"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"
)

// TestServedCtxSize → REQ-PERF-009: the client reads the running server's ACTUAL context window from
// llama.cpp's /props, so OpenCode can count tokens against the real window (not aegis's intended value).
// It accepts n_ctx at the top level or under default_generation_settings, and returns 0 when /props is
// absent so the caller falls back to the resolver.
func TestServedCtxSize(t *testing.T) {
	cases := []struct {
		name string
		body string
		code int
		want int
	}{
		{"top-level n_ctx", `{"n_ctx": 32768}`, 200, 32768},
		{"nested n_ctx", `{"default_generation_settings": {"n_ctx": 16384}}`, 200, 16384},
		{"no props endpoint", `not found`, 404, 0},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				if r.URL.Path != "/props" {
					http.NotFound(w, r)
					return
				}
				w.WriteHeader(tc.code)
				_, _ = w.Write([]byte(tc.body))
			}))
			defer srv.Close()

			c, err := NewClient(srv.URL) // httptest binds 127.0.0.1 — loopback, passes the guard
			if err != nil {
				t.Fatalf("NewClient: %v", err)
			}
			ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
			defer cancel()
			got, err := c.ServedCtxSize(ctx)
			if tc.want == 0 {
				// a 404 surfaces an error; the caller treats <512 / error as "fall back to the resolver"
				if got >= 512 {
					t.Errorf("absent /props must not yield a usable ctx; got %d (err=%v)", got, err)
				}
				return
			}
			if err != nil {
				t.Fatalf("ServedCtxSize: %v", err)
			}
			if got != tc.want {
				t.Errorf("ServedCtxSize = %d, want %d", got, tc.want)
			}
		})
	}
}
