// Command api starts the production Workplane M1 service.
package main

import (
	"context"
	"encoding/json"
	"errors"
	"log"
	"net/http"
	"os"
	"os/signal"
	"path/filepath"
	"syscall"
	"time"

	"github.com/agent-team-project/kensho-projects/projects/workplane/internal/api/generated"
	"github.com/agent-team-project/kensho-projects/projects/workplane/internal/app"
)

func main() {
	if len(os.Args) == 2 && os.Args[1] == "healthcheck" {
		client := http.Client{Timeout: 2 * time.Second}
		response, err := client.Get("http://127.0.0.1:8080/readyz")
		if err != nil || response.StatusCode != http.StatusOK {
			os.Exit(1)
		}
		_ = response.Body.Close()
		return
	}
	if len(os.Args) == 2 && os.Args[1] == "outbox" {
		runOutbox()
		return
	}
	config, err := app.ConfigFromEnv()
	if err != nil {
		log.Fatal(err)
	}
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	service, err := app.NewService(ctx, config)
	if err != nil {
		log.Fatal(err)
	}
	defer service.Close()

	mux := http.NewServeMux()
	mux.HandleFunc("GET /healthz", func(writer http.ResponseWriter, _ *http.Request) {
		writer.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(writer).Encode(map[string]any{
			"status": "ok", "build_stage": app.BuildStage, "capabilities": app.Capabilities,
		})
	})
	mux.HandleFunc("GET /readyz", func(writer http.ResponseWriter, request *http.Request) {
		if err := service.Ready(request.Context()); err != nil {
			http.Error(writer, "not ready", http.StatusServiceUnavailable)
			return
		}
		writer.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(writer).Encode(map[string]string{"status": "ready"})
	})
	generated.RegisterHandlers(mux, service)
	registerWeb(mux, os.Getenv("WORKPLANE_WEB_ROOT"))

	address := os.Getenv("WORKPLANE_LISTEN_ADDR")
	if address == "" {
		address = ":8080"
	}
	server := &http.Server{
		Addr: address, Handler: securityHeaders(mux), ReadHeaderTimeout: 5 * time.Second,
		ReadTimeout: 15 * time.Second, WriteTimeout: 30 * time.Second, IdleTimeout: 60 * time.Second,
	}
	stop := make(chan os.Signal, 1)
	signal.Notify(stop, syscall.SIGINT, syscall.SIGTERM)
	go func() {
		<-stop
		shutdownCtx, shutdownCancel := context.WithTimeout(context.Background(), 10*time.Second)
		defer shutdownCancel()
		_ = server.Shutdown(shutdownCtx)
	}()
	log.Printf("Workplane %s listening on %s", app.BuildStage, address)
	if err := server.ListenAndServe(); err != nil && !errors.Is(err, http.ErrServerClosed) {
		log.Fatal(err)
	}
}

func runOutbox() {
	databaseURL := os.Getenv("WORKPLANE_DATABASE_URL")
	if databaseURL == "" {
		log.Fatal("WORKPLANE_DATABASE_URL is required")
	}
	store, err := app.NewDurableStore(databaseURL)
	if err != nil {
		log.Fatal(err)
	}
	defer store.Close()
	ctx, stop := signal.NotifyContext(context.Background(), syscall.SIGINT, syscall.SIGTERM)
	defer stop()
	readyCtx, cancel := context.WithTimeout(ctx, 30*time.Second)
	defer cancel()
	if err := store.Ping(readyCtx); err != nil {
		log.Fatal(err)
	}
	worker, _ := os.Hostname()
	if worker == "" {
		worker = "workplane-outbox"
	}
	log.Printf("Workplane %s outbox consumer started as %s", app.BuildStage, worker)
	if err := store.RunOutboxLoop(ctx, "projection-v1", worker, 30*time.Second, 100*time.Millisecond); err != nil {
		log.Fatal(err)
	}
}

func registerWeb(mux *http.ServeMux, root string) {
	if root == "" {
		return
	}
	index := filepath.Join(root, "index.html")
	assets := http.FileServer(http.Dir(root))
	mux.HandleFunc("GET /", func(writer http.ResponseWriter, request *http.Request) {
		if request.URL.Path != "/" {
			if _, err := os.Stat(filepath.Join(root, filepath.Clean(request.URL.Path))); err == nil {
				assets.ServeHTTP(writer, request)
				return
			}
		}
		http.ServeFile(writer, request, index)
	})
}

func securityHeaders(next http.Handler) http.Handler {
	return http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		writer.Header().Set("X-Content-Type-Options", "nosniff")
		writer.Header().Set("Referrer-Policy", "no-referrer")
		writer.Header().Set("Permissions-Policy", "camera=(), microphone=(), geolocation=()")
		writer.Header().Set("Content-Security-Policy", "default-src 'self'; connect-src 'self'; img-src 'self' data:; style-src 'self'; script-src 'self'; object-src 'none'; frame-ancestors 'none'; base-uri 'none'")
		next.ServeHTTP(writer, request)
	})
}
