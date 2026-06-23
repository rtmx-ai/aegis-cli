package serving

import (
	"context"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"
)

func TestHealthOK(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path == "/health" {
			w.WriteHeader(http.StatusOK)
			return
		}
		w.WriteHeader(http.StatusNotFound)
	}))
	defer srv.Close()

	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()
	if err := Health(ctx, Endpoint{URL: srv.URL, Client: srv.Client()}); err != nil {
		t.Fatalf("loopback health probe should pass: %v", err)
	}
}

func TestHealthRejectsNonLoopback(t *testing.T) {
	err := Health(context.Background(), Endpoint{URL: "http://example.com:8080"})
	if err == nil {
		t.Fatal("non-loopback endpoint must be rejected before any request")
	}
}

func TestHealthNon200(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusServiceUnavailable)
	}))
	defer srv.Close()
	if err := Health(context.Background(), Endpoint{URL: srv.URL, Client: srv.Client()}); err == nil {
		t.Fatal("non-200 health must error")
	}
}
