// Command api starts the M0 Workplane API skeleton.
package main

import (
	"encoding/json"
	"log"
	"net/http"
	"os"

	"github.com/agent-team-project/kensho-projects/projects/workplane/internal/app"
)

func main() {
	mux := http.NewServeMux()
	mux.HandleFunc("GET /healthz", func(writer http.ResponseWriter, _ *http.Request) {
		writer.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(writer).Encode(map[string]any{
			"status":       "ok",
			"build_stage":  app.BuildStage,
			"capabilities": app.Capabilities,
		})
	})

	address := os.Getenv("WORKPLANE_LISTEN_ADDR")
	if address == "" {
		address = ":8080"
	}
	log.Printf("Workplane %s listening on %s", app.BuildStage, address)
	server := &http.Server{Addr: address, Handler: mux, ReadHeaderTimeout: 5_000_000_000}
	if err := server.ListenAndServe(); err != nil && err != http.ErrServerClosed {
		log.Fatal(err)
	}
}
