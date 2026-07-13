package generated

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/cookiejar"
	"net/http/httptest"
	"strings"
	"testing"
)

type adapterTestHandler struct {
	Handler
	loginResponse          Response
	createProjectRequests  chan Request
	recordDecisionResponse Response
}

func (handler *adapterTestHandler) CreateProject(_ context.Context, request Request) (Response, error) {
	if handler.createProjectRequests != nil {
		handler.createProjectRequests <- request
	}
	return Response{Status: http.StatusCreated, Body: map[string]string{"id": "project-id"}}, nil
}

func (*adapterTestHandler) GetProject(context.Context, Request) (Response, error) {
	return Response{Status: http.StatusOK, Body: map[string]string{}}, nil
}

func (*adapterTestHandler) ListProjectActivity(context.Context, Request) (Response, error) {
	return Response{Status: http.StatusOK, Body: []any{}}, nil
}

func (handler *adapterTestHandler) Login(context.Context, Request) (Response, error) {
	return handler.loginResponse, nil
}

func (handler *adapterTestHandler) RecordDecision(context.Context, Request) (Response, error) {
	return handler.recordDecisionResponse, nil
}

func (*adapterTestHandler) ActivateProject(context.Context, Request) (Response, error) {
	return Response{Status: http.StatusOK, Body: map[string]string{}}, nil
}
func (*adapterTestHandler) HoldProject(context.Context, Request) (Response, error) {
	return Response{Status: http.StatusOK, Body: map[string]string{}}, nil
}
func (*adapterTestHandler) ResumeProject(context.Context, Request) (Response, error) {
	return Response{Status: http.StatusOK, Body: map[string]string{}}, nil
}
func (*adapterTestHandler) PromoteProject(context.Context, Request) (Response, error) {
	return Response{Status: http.StatusOK, Body: map[string]string{}}, nil
}
func (*adapterTestHandler) CreateDeliverable(context.Context, Request) (Response, error) {
	return Response{Status: http.StatusCreated, Body: map[string]string{}}, nil
}
func (*adapterTestHandler) GetDeliverable(context.Context, Request) (Response, error) {
	return Response{Status: http.StatusOK, Body: map[string]string{}}, nil
}
func (*adapterTestHandler) ReviseDeliverable(context.Context, Request) (Response, error) {
	return Response{Status: http.StatusOK, Body: map[string]string{}}, nil
}
func (*adapterTestHandler) ListDeliverables(context.Context, Request) (Response, error) {
	return Response{Status: http.StatusOK, Body: []any{}}, nil
}
func (*adapterTestHandler) ReforecastProject(context.Context, Request) (Response, error) {
	return Response{Status: http.StatusCreated, Body: map[string]string{}}, nil
}
func (*adapterTestHandler) ReforecastDeliverable(context.Context, Request) (Response, error) {
	return Response{Status: http.StatusCreated, Body: map[string]string{}}, nil
}
func (*adapterTestHandler) ListProjectForecasts(context.Context, Request) (Response, error) {
	return Response{Status: http.StatusOK, Body: []any{}}, nil
}
func (*adapterTestHandler) ListDeliverableForecasts(context.Context, Request) (Response, error) {
	return Response{Status: http.StatusOK, Body: []any{}}, nil
}
func (*adapterTestHandler) SetProjectTarget(context.Context, Request) (Response, error) {
	return Response{Status: http.StatusCreated, Body: map[string]string{}}, nil
}
func (*adapterTestHandler) SetProjectDeadline(context.Context, Request) (Response, error) {
	return Response{Status: http.StatusCreated, Body: map[string]string{}}, nil
}

func TestHumanSessionBootstrap(t *testing.T) {
	t.Parallel()

	const csrfToken = "csrf-token-with-at-least-thirty-two-bytes"
	handler := &adapterTestHandler{
		loginResponse: Response{
			Status: http.StatusOK,
			Headers: ResponseHeaders{
				SetCookie: "workplane_session=opaque-session; Path=/; HttpOnly; SameSite=Lax",
			},
			Body: Session{
				ActorID:   "00000000-0000-4000-8000-000000000001",
				ActorKind: "human",
				ExpiresAt: "2026-07-13T00:00:00Z",
				CSRFToken: csrfToken,
			},
		},
		createProjectRequests: make(chan Request, 1),
	}
	mux := http.NewServeMux()
	RegisterHandlers(mux, handler)
	server := httptest.NewServer(mux)
	t.Cleanup(server.Close)

	jar, err := cookiejar.New(nil)
	if err != nil {
		t.Fatalf("create cookie jar: %v", err)
	}
	client := server.Client()
	client.Jar = jar

	loginResponse, err := client.Post(
		server.URL+"/api/v1/session/login",
		"application/json",
		strings.NewReader(`{"email":"human@example.test","password":"secret"}`),
	)
	if err != nil {
		t.Fatalf("login request: %v", err)
	}
	defer loginResponse.Body.Close()
	if got := loginResponse.Header.Get("Set-Cookie"); !strings.HasPrefix(got, "workplane_session=") {
		t.Fatalf("login response did not issue declared session cookie: %q", got)
	}
	var session Session
	if err := json.NewDecoder(loginResponse.Body).Decode(&session); err != nil {
		t.Fatalf("decode login response: %v", err)
	}
	if session.CSRFToken != csrfToken {
		t.Fatalf("login response CSRF token = %q, want %q", session.CSRFToken, csrfToken)
	}

	mutation, err := http.NewRequest(
		http.MethodPost,
		server.URL+"/api/v1/orgs/00000000-0000-4000-8000-000000000010/projects",
		strings.NewReader(`{"title":"walking slice"}`),
	)
	if err != nil {
		t.Fatalf("create mutation request: %v", err)
	}
	mutation.Header.Set("Content-Type", "application/json")
	mutation.Header.Set("Idempotency-Key", "human-bootstrap-0001")
	mutation.Header.Set("X-CSRF-Token", session.CSRFToken)
	mutationResponse, err := client.Do(mutation)
	if err != nil {
		t.Fatalf("cookie-authenticated mutation: %v", err)
	}
	defer mutationResponse.Body.Close()

	adapted := <-handler.createProjectRequests
	if adapted.Security.SessionCookie != "opaque-session" {
		t.Fatalf("adapted session cookie = %q, want opaque-session", adapted.Security.SessionCookie)
	}
	if adapted.Security.CSRFToken != csrfToken {
		t.Fatalf("adapted CSRF token = %q, want %q", adapted.Security.CSRFToken, csrfToken)
	}
}

func TestRecordDecisionCanReturnContractETag(t *testing.T) {
	t.Parallel()

	handler := &adapterTestHandler{
		recordDecisionResponse: Response{
			Status:  http.StatusCreated,
			Headers: ResponseHeaders{ETag: VersionETag(`"8"`)},
			Body:    map[string]string{"id": "decision-id"},
		},
	}
	mux := http.NewServeMux()
	RegisterHandlers(mux, handler)

	request := httptest.NewRequest(
		http.MethodPost,
		"/api/v1/projects/00000000-0000-4000-8000-000000000020/decisions",
		strings.NewReader(`{"kind":"continue"}`),
	)
	recorder := httptest.NewRecorder()
	mux.ServeHTTP(recorder, request)

	if got := recorder.Header().Get("ETag"); got != `"8"` {
		t.Fatalf("recordDecision ETag = %q, want %q", got, `"8"`)
	}
}
