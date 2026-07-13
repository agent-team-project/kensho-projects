package app

import (
	"context"
	"database/sql"
	"encoding/json"
	"fmt"
	"net/http"
	"regexp"
	"strconv"
	"strings"
	"time"
	"unicode/utf8"

	"github.com/agent-team-project/kensho-projects/projects/workplane/internal/api/generated"
)

var (
	uuidPattern        = regexp.MustCompile(`^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$`)
	versionETagPattern = regexp.MustCompile(`^"([1-9][0-9]*)"$`)
)

func (service *Service) Login(ctx context.Context, request generated.Request) (generated.Response, error) {
	rid := requestID()
	input, err := decodeStrict[generated.LoginRequest](request.Body)
	if err != nil || !strings.Contains(input.Email, "@") ||
		utf8.RuneCountInString(input.Email) < generated.LoginRequestEmailMinLength ||
		utf8.RuneCountInString(input.Email) > generated.LoginRequestEmailMaxLength ||
		utf8.RuneCountInString(input.Password) < generated.LoginRequestPasswordMinLength ||
		utf8.RuneCountInString(input.Password) > generated.LoginRequestPasswordMaxLength {
		return problem(http.StatusBadRequest, "invalid_request", "Invalid request", "The login request does not match the public contract.", rid), nil
	}
	input.Email = strings.ToLower(strings.TrimSpace(input.Email))
	var actorID, actorKind, status, encodedPassword string
	err = service.db.QueryRowContext(ctx, `SELECT p.id,p.kind,p.status,c.password_hash
		FROM human_credentials c JOIN principals p ON p.id=c.principal_id WHERE c.email=$1`, input.Email).
		Scan(&actorID, &actorKind, &status, &encodedPassword)
	known := err == nil && actorKind == "human" && status == "active"
	if !known {
		encodedPassword = service.dummyPasswordHash
	}
	passwordOK := verifyPassword(encodedPassword, input.Password)
	if !known || !passwordOK {
		if known {
			_, _ = service.db.ExecContext(ctx, `UPDATE human_credentials SET failed_attempts=failed_attempts+1,last_failed_at=CURRENT_TIMESTAMP WHERE principal_id=$1`, actorID)
		}
		service.audit(ctx, "login.failure", rid, nil, map[string]any{"category": "invalid_credentials"})
		return problem(http.StatusUnauthorized, "unauthenticated", "Login failed", "The supplied credentials are invalid.", rid), nil
	}

	sessionID, err := newUUID()
	if err != nil {
		return problem(http.StatusServiceUnavailable, "service_unavailable", "Login unavailable", "Please retry the request.", rid), nil
	}
	sessionToken, err := randomToken("wps_", 32)
	if err != nil {
		return problem(http.StatusServiceUnavailable, "service_unavailable", "Login unavailable", "Please retry the request.", rid), nil
	}
	csrfToken, err := randomToken("wpc_", 32)
	if err != nil {
		return problem(http.StatusServiceUnavailable, "service_unavailable", "Login unavailable", "Please retry the request.", rid), nil
	}
	expires := service.now().Add(service.config.SessionTTL)
	tx, err := service.db.BeginTx(ctx, nil)
	if err != nil {
		return problem(http.StatusServiceUnavailable, "service_unavailable", "Login unavailable", "Please retry the request.", rid), nil
	}
	defer tx.Rollback()
	if _, err := tx.ExecContext(ctx, `UPDATE human_sessions SET revoked_at=CURRENT_TIMESTAMP WHERE principal_id=$1 AND revoked_at IS NULL`, actorID); err != nil {
		return problem(http.StatusServiceUnavailable, "service_unavailable", "Login unavailable", "Please retry the request.", rid), nil
	}
	if _, err := tx.ExecContext(ctx, `INSERT INTO human_sessions
		(id,principal_id,token_hash,csrf_hash,expires_at) VALUES ($1,$2,$3,$4,$5)`,
		sessionID, actorID, keyedHash(service.config.TokenHashKey, sessionToken), keyedHash(service.config.TokenHashKey, csrfToken), expires); err != nil {
		return problem(http.StatusServiceUnavailable, "service_unavailable", "Login unavailable", "Please retry the request.", rid), nil
	}
	if _, err := tx.ExecContext(ctx, `UPDATE human_credentials SET failed_attempts=0,last_failed_at=NULL WHERE principal_id=$1`, actorID); err != nil {
		return problem(http.StatusServiceUnavailable, "service_unavailable", "Login unavailable", "Please retry the request.", rid), nil
	}
	if err := tx.Commit(); err != nil {
		return problem(http.StatusServiceUnavailable, "service_unavailable", "Login unavailable", "Please retry the request.", rid), nil
	}
	cookie := fmt.Sprintf("workplane_session=%s; Path=/; HttpOnly; SameSite=Lax", sessionToken)
	if service.config.CookieSecure {
		cookie += "; Secure"
	}
	service.audit(ctx, "login.success", rid, &Actor{ID: actorID}, map[string]any{"session_id": sessionID})
	return generated.Response{
		Status:  http.StatusOK,
		Headers: generated.ResponseHeaders{SetCookie: cookie, XRequestID: rid},
		Body:    generated.Session{ActorID: actorID, ActorKind: "human", ExpiresAt: expires.Format(timeFormat), CSRFToken: csrfToken},
	}, nil
}

const timeFormat = "2006-01-02T15:04:05.000000Z"

func (service *Service) CreateProject(ctx context.Context, request generated.Request) (generated.Response, error) {
	actor, authResponse, ok := service.authenticate(ctx, request, "project.create", "")
	if !ok {
		return authResponse, nil
	}
	rid := authResponse.Headers.XRequestID
	if denied, ok := service.requireHumanMutation(request, actor, rid); !ok {
		return denied, nil
	}
	orgID := request.HTTPRequest.PathValue("org_id")
	if !uuidPattern.MatchString(orgID) {
		return problem(http.StatusBadRequest, "invalid_request", "Invalid organization identifier", "The organization identifier is malformed.", rid), nil
	}
	if denied, ok := service.authorizeOrganization(actor, orgID, "project.create", rid); !ok {
		return denied, nil
	}
	if utf8.RuneCountInString(request.IdempotencyKey) < generated.IdempotencyKeyMinLength ||
		utf8.RuneCountInString(request.IdempotencyKey) > generated.IdempotencyKeyMaxLength {
		return problem(http.StatusBadRequest, "invalid_request", "Idempotency key required", "Idempotency-Key must contain 16 to 128 characters.", rid), nil
	}
	input, err := decodeStrict[generated.CreateExplorationProject](request.Body)
	if err != nil ||
		!validText(input.Title, generated.CreateExplorationProjectTitleMinLength, generated.CreateExplorationProjectTitleMaxLength) ||
		!validText(input.Outcome, generated.CreateExplorationProjectOutcomeMinLength, generated.CreateExplorationProjectOutcomeMaxLength) ||
		!validText(input.Hypothesis, generated.CreateExplorationProjectHypothesisMinLength, generated.CreateExplorationProjectHypothesisMaxLength) ||
		!validText(input.Falsifier, generated.CreateExplorationProjectFalsifierMinLength, generated.CreateExplorationProjectFalsifierMaxLength) ||
		!validText(input.ExperimentBound, generated.CreateExplorationProjectExperimentBoundMinLength, generated.CreateExplorationProjectExperimentBoundMaxLength) {
		return problem(http.StatusBadRequest, "invalid_request", "Invalid project contract", "The exploration project does not match the public contract.", rid), nil
	}
	criteria, valid := normalizeStrings(input.DecisionCriteria,
		generated.CreateExplorationProjectDecisionCriteriaMinItems,
		generated.CreateExplorationProjectDecisionCriteriaMaxItems,
		generated.CreateExplorationProjectDecisionCriteriaItemMinLength,
		generated.CreateExplorationProjectDecisionCriteriaItemMaxLength)
	if !valid {
		return problem(http.StatusBadRequest, "invalid_request", "Invalid project contract", "Decision criteria exceed the public limits.", rid), nil
	}
	input.Title = strings.TrimSpace(input.Title)
	input.Outcome = strings.TrimSpace(input.Outcome)
	input.Hypothesis = strings.TrimSpace(input.Hypothesis)
	input.Falsifier = strings.TrimSpace(input.Falsifier)
	input.ExperimentBound = strings.TrimSpace(input.ExperimentBound)
	input.DecisionCriteria = criteria
	canonical := canonicalJSON(input)
	hash := requestHash(canonical)

	tx, err := service.db.BeginTx(ctx, &sql.TxOptions{Isolation: sql.LevelSerializable})
	if err != nil {
		return serviceUnavailable(rid), nil
	}
	defer tx.Rollback()
	if _, err := tx.ExecContext(ctx, `SELECT pg_advisory_xact_lock(hashtextextended($1,0))`, orgID+actor.ID+"createProject"+request.IdempotencyKey); err != nil {
		return serviceUnavailable(rid), nil
	}
	if replay, found, conflict := replayIdempotency(ctx, tx, orgID, actor.ID, "createProject", request.IdempotencyKey, hash); found {
		if conflict {
			return problem(http.StatusConflict, "idempotency_conflict", "Idempotency key conflict", "The key was already used with a different request.", rid), nil
		}
		return replay, nil
	}

	projectID, _ := newUUID()
	eventID, _ := newUUID()
	commandID, _ := newUUID()
	now := service.now().UTC().Truncate(time.Microsecond)
	project := Project{ID: projectID, OrganizationID: orgID, Title: input.Title, Outcome: input.Outcome,
		Mode: "exploration", State: "proposed", Version: 1, Hypothesis: input.Hypothesis,
		Falsifier: input.Falsifier, DecisionCriteria: criteria, ExperimentBound: input.ExperimentBound}
	criteriaJSON := canonicalJSON(criteria)
	if _, err := tx.ExecContext(ctx, `INSERT INTO projects
		(id,organization_id,title,outcome,mode,state,version,hypothesis,falsifier,decision_criteria,experiment_bound,created_by,created_at,updated_at)
		VALUES ($1,$2,$3,$4,'exploration','proposed',1,$5,$6,$7,$8,$9,$10,$10)`,
		projectID, orgID, input.Title, input.Outcome, input.Hypothesis, input.Falsifier, criteriaJSON, input.ExperimentBound, actor.ID, now); err != nil {
		return serviceUnavailable(rid), nil
	}
	if service.config.FaultInjection && request.HTTPRequest.Header.Get("X-Workplane-Fault") == "after-project" {
		return serviceUnavailable(rid), nil
	}
	if err := insertEvent(ctx, tx, eventID, orgID, projectID, 1, "project.created", actor, commandID, rid, now, project,
		service.config.FaultInjection && request.HTTPRequest.Header.Get("X-Workplane-Fault") == "after-event"); err != nil {
		return serviceUnavailable(rid), nil
	}
	body := canonicalJSON(project)
	if err := storeIdempotency(ctx, tx, orgID, actor.ID, "createProject", request.IdempotencyKey, hash, http.StatusCreated, body, "", rid, []string{eventID}, now); err != nil {
		return serviceUnavailable(rid), nil
	}
	if err := tx.Commit(); err != nil {
		return serviceUnavailable(rid), nil
	}
	return generated.Response{Status: http.StatusCreated, Headers: generated.ResponseHeaders{XRequestID: rid}, Body: project}, nil
}

func (service *Service) GetProject(ctx context.Context, request generated.Request) (generated.Response, error) {
	projectID := request.HTTPRequest.PathValue("project_id")
	actor, authResponse, ok := service.authenticate(ctx, request, "project.read", projectID)
	if !ok {
		return authResponse, nil
	}
	rid := authResponse.Headers.XRequestID
	if len(strings.TrimSpace(string(request.Body))) > 0 {
		return problem(http.StatusBadRequest, "invalid_request", "Invalid request", "GET requests cannot contain a body.", rid), nil
	}
	if !uuidPattern.MatchString(projectID) {
		return problem(http.StatusNotFound, "not_found", "Resource not found", "The requested resource is not available.", rid), nil
	}
	project, err := scanProject(service.db.QueryRowContext(ctx, selectProject+" WHERE id=$1 AND organization_id=$2", projectID, actor.OrganizationID))
	if err != nil {
		return problem(http.StatusNotFound, "not_found", "Resource not found", "The requested resource is not available.", rid), nil
	}
	if denied, ok := service.authorizeOrganization(actor, project.OrganizationID, "project.read", rid); !ok {
		return denied, nil
	}
	return generated.Response{Status: http.StatusOK, Headers: generated.ResponseHeaders{XRequestID: rid}, Body: project}, nil
}

func (service *Service) RecordDecision(ctx context.Context, request generated.Request) (generated.Response, error) {
	projectID := request.HTTPRequest.PathValue("project_id")
	actor, authResponse, ok := service.authenticate(ctx, request, "decision.record", projectID)
	if !ok {
		return authResponse, nil
	}
	rid := authResponse.Headers.XRequestID
	if denied, ok := service.requireHumanMutation(request, actor, rid); !ok {
		return denied, nil
	}
	if !uuidPattern.MatchString(projectID) {
		return problem(http.StatusNotFound, "not_found", "Resource not found", "The requested resource is not available.", rid), nil
	}
	if utf8.RuneCountInString(request.IdempotencyKey) < generated.IdempotencyKeyMinLength ||
		utf8.RuneCountInString(request.IdempotencyKey) > generated.IdempotencyKeyMaxLength {
		return problem(http.StatusBadRequest, "invalid_request", "Idempotency key required", "Idempotency-Key must contain 16 to 128 characters.", rid), nil
	}
	match := versionETagPattern.FindStringSubmatch(string(request.ExpectedVersion))
	if len(match) != 2 {
		return problem(http.StatusPreconditionRequired, "version_conflict", "Expected version required", "If-Match must contain the quoted aggregate version.", rid), nil
	}
	expectedVersion, _ := strconv.ParseInt(match[1], 10, 64)
	input, err := decodeStrict[generated.RecordDecision](request.Body)
	if err != nil || input.Kind != "continue" ||
		!validText(input.Question, generated.RecordDecisionQuestionMinLength, generated.RecordDecisionQuestionMaxLength) ||
		!validText(input.Choice, generated.RecordDecisionChoiceMinLength, generated.RecordDecisionChoiceMaxLength) ||
		!validText(input.Rationale, generated.RecordDecisionRationaleMinLength, generated.RecordDecisionRationaleMaxLength) {
		return problem(http.StatusBadRequest, "invalid_request", "Invalid decision", "M1 accepts the complete universal continue-decision shape.", rid), nil
	}
	var valid bool
	if input.Alternatives, valid = normalizeStrings(input.Alternatives, 0,
		generated.RecordDecisionAlternativesMaxItems, 0, generated.RecordDecisionAlternativesItemMaxLength); !valid {
		return problem(http.StatusBadRequest, "invalid_request", "Invalid decision", "Decision alternatives exceed the public limits.", rid), nil
	}
	if input.Evidence, valid = normalizeStrings(input.Evidence, 0,
		generated.RecordDecisionEvidenceMaxItems, 0, generated.RecordDecisionEvidenceItemMaxLength); !valid {
		return problem(http.StatusBadRequest, "invalid_request", "Invalid decision", "Decision evidence exceeds the public limits.", rid), nil
	}
	if input.Consequences, valid = normalizeStrings(input.Consequences, 0,
		generated.RecordDecisionConsequencesMaxItems, 0, generated.RecordDecisionConsequencesItemMaxLength); !valid {
		return problem(http.StatusBadRequest, "invalid_request", "Invalid decision", "Decision consequences exceed the public limits.", rid), nil
	}
	input.Question, input.Choice, input.Rationale = strings.TrimSpace(input.Question), strings.TrimSpace(input.Choice), strings.TrimSpace(input.Rationale)
	canonical := canonicalJSON(input)
	hash := requestHash(canonical, []byte(request.ExpectedVersion))

	tx, err := service.db.BeginTx(ctx, &sql.TxOptions{Isolation: sql.LevelSerializable})
	if err != nil {
		return serviceUnavailable(rid), nil
	}
	defer tx.Rollback()
	if _, err := tx.ExecContext(ctx, `SELECT pg_advisory_xact_lock(hashtextextended($1,0))`, actor.OrganizationID+actor.ID+"recordDecision"+request.IdempotencyKey); err != nil {
		return serviceUnavailable(rid), nil
	}
	if replay, found, conflict := replayIdempotency(ctx, tx, actor.OrganizationID, actor.ID, "recordDecision", request.IdempotencyKey, hash); found {
		if conflict {
			return problem(http.StatusConflict, "idempotency_conflict", "Idempotency key conflict", "The key was already used with a different request.", rid), nil
		}
		return replay, nil
	}
	project, err := scanProject(tx.QueryRowContext(ctx, selectProject+" WHERE id=$1 AND organization_id=$2 FOR UPDATE", projectID, actor.OrganizationID))
	if err != nil {
		return problem(http.StatusNotFound, "not_found", "Resource not found", "The requested resource is not available.", rid), nil
	}
	if denied, ok := service.authorizeOrganization(actor, project.OrganizationID, "decision.record", rid); !ok {
		return denied, nil
	}
	if project.Mode != "exploration" || (project.State != "proposed" && project.State != "active") {
		return problem(http.StatusConflict, "invariant_violation", "Decision not allowed", "A continue decision requires a mutable exploration project.", rid), nil
	}
	if project.Version != expectedVersion {
		return problem(http.StatusConflict, "version_conflict", "Project version changed", fmt.Sprintf("Expected version %d; current version is %d.", expectedVersion, project.Version), rid), nil
	}

	decisionID, _ := newUUID()
	eventID, _ := newUUID()
	commandID, _ := newUUID()
	now := service.now().UTC().Truncate(time.Microsecond)
	newVersion := project.Version + 1
	decision := Decision{ID: decisionID, ProjectID: projectID, ActorID: actor.ID, ActorKind: actor.Kind,
		PrincipalID: actor.PrincipalID, RecordedAt: now.Format(timeFormat), Kind: input.Kind, Question: input.Question,
		Choice: input.Choice, Alternatives: input.Alternatives, Rationale: input.Rationale, Evidence: input.Evidence, Consequences: input.Consequences}
	if _, err := tx.ExecContext(ctx, `UPDATE projects SET version=$1,updated_at=$2 WHERE id=$3 AND version=$4`, newVersion, now, projectID, project.Version); err != nil {
		return serviceUnavailable(rid), nil
	}
	if _, err := tx.ExecContext(ctx, `INSERT INTO decisions
		(id,organization_id,project_id,kind,question,choice,alternatives,rationale,evidence,consequences,actor_id,recorded_at)
		VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)`,
		decisionID, actor.OrganizationID, projectID, input.Kind, input.Question, input.Choice,
		canonicalJSON(input.Alternatives), input.Rationale, canonicalJSON(input.Evidence), canonicalJSON(input.Consequences), actor.ID, now); err != nil {
		return serviceUnavailable(rid), nil
	}
	if service.config.FaultInjection && request.HTTPRequest.Header.Get("X-Workplane-Fault") == "after-decision" {
		return serviceUnavailable(rid), nil
	}
	if err := insertEvent(ctx, tx, eventID, actor.OrganizationID, projectID, newVersion, "decision.recorded", actor, commandID, rid, now, decision,
		service.config.FaultInjection && request.HTTPRequest.Header.Get("X-Workplane-Fault") == "after-event"); err != nil {
		return serviceUnavailable(rid), nil
	}
	etag := fmt.Sprintf(`"%d"`, newVersion)
	body := canonicalJSON(decision)
	if err := storeIdempotency(ctx, tx, actor.OrganizationID, actor.ID, "recordDecision", request.IdempotencyKey, hash,
		http.StatusCreated, body, etag, rid, []string{eventID}, now); err != nil {
		return serviceUnavailable(rid), nil
	}
	if err := tx.Commit(); err != nil {
		return serviceUnavailable(rid), nil
	}
	return generated.Response{Status: http.StatusCreated, Headers: generated.ResponseHeaders{ETag: generated.VersionETag(etag), XRequestID: rid}, Body: decision}, nil
}

func (service *Service) ListProjectActivity(ctx context.Context, request generated.Request) (generated.Response, error) {
	projectID := request.HTTPRequest.PathValue("project_id")
	actor, authResponse, ok := service.authenticate(ctx, request, "project.read", projectID)
	if !ok {
		return authResponse, nil
	}
	rid := authResponse.Headers.XRequestID
	if len(strings.TrimSpace(string(request.Body))) > 0 {
		return problem(http.StatusBadRequest, "invalid_request", "Invalid request", "GET requests cannot contain a body.", rid), nil
	}
	if !uuidPattern.MatchString(projectID) {
		return problem(http.StatusNotFound, "not_found", "Resource not found", "The requested resource is not available.", rid), nil
	}
	var exists bool
	if err := service.db.QueryRowContext(ctx, `SELECT true FROM projects WHERE id=$1 AND organization_id=$2`, projectID, actor.OrganizationID).Scan(&exists); err != nil {
		return problem(http.StatusNotFound, "not_found", "Resource not found", "The requested resource is not available.", rid), nil
	}
	rows, err := service.db.QueryContext(ctx, `SELECT event_id,event_type,actor_id,actor_kind,principal_id,
		aggregate_version,command_id,request_id,occurred_at FROM domain_events
		WHERE organization_id=$1 AND aggregate_type='project' AND aggregate_id=$2 ORDER BY sequence`, actor.OrganizationID, projectID)
	if err != nil {
		return serviceUnavailable(rid), nil
	}
	defer rows.Close()
	activity := make([]Activity, 0)
	for rows.Next() {
		var item Activity
		var occurredAt time.Time
		if err := rows.Scan(&item.EventID, &item.EventType, &item.ActorID, &item.ActorKind, &item.PrincipalID,
			&item.AggregateVersion, &item.CommandID, &item.RequestID, &occurredAt); err != nil {
			return serviceUnavailable(rid), nil
		}
		item.OccurredAt = occurredAt.UTC().Format(timeFormat)
		activity = append(activity, item)
	}
	if err := rows.Err(); err != nil {
		return serviceUnavailable(rid), nil
	}
	return generated.Response{Status: http.StatusOK, Headers: generated.ResponseHeaders{XRequestID: rid}, Body: activity}, nil
}

func replayIdempotency(ctx context.Context, tx *sql.Tx, orgID, actorID, operation, key string, hash []byte) (generated.Response, bool, bool) {
	var storedHash, body []byte
	var status int
	var etag sql.NullString
	var rid string
	err := tx.QueryRowContext(ctx, `SELECT request_hash,response_status,response_body,response_etag,response_request_id
		FROM idempotency_results WHERE organization_id=$1 AND actor_id=$2 AND operation_id=$3 AND idempotency_key=$4`,
		orgID, actorID, operation, key).Scan(&storedHash, &status, &body, &etag, &rid)
	if err == sql.ErrNoRows {
		return generated.Response{}, false, false
	}
	if err != nil {
		return generated.Response{}, true, true
	}
	if !hashesEqual(storedHash, hash) {
		return generated.Response{}, true, true
	}
	if !json.Valid(body) {
		return generated.Response{}, true, true
	}
	response := generated.Response{Status: status, Headers: generated.ResponseHeaders{XRequestID: rid}, Body: json.RawMessage(body)}
	if etag.Valid {
		response.Headers.ETag = generated.VersionETag(etag.String)
	}
	return response, true, false
}

func storeIdempotency(ctx context.Context, tx *sql.Tx, orgID, actorID, operation, key string, hash []byte,
	status int, body []byte, etag, rid string, eventIDs []string, createdAt any) error {
	var nullableETag any
	if etag != "" {
		nullableETag = etag
	}
	_, err := tx.ExecContext(ctx, `INSERT INTO idempotency_results
		(organization_id,actor_id,operation_id,idempotency_key,request_hash,response_status,response_body,response_etag,response_request_id,event_ids,created_at)
		VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)`,
		orgID, actorID, operation, key, hash, status, body, nullableETag, rid, "{"+strings.Join(eventIDs, ",")+"}", createdAt)
	return err
}

func insertEvent(ctx context.Context, tx *sql.Tx, eventID, orgID, projectID string, version int64, eventType string,
	actor Actor, commandID, rid string, occurredAt time.Time, payload any, failAfterEvent bool) error {
	encoded := canonicalJSON(payload)
	_, err := tx.ExecContext(ctx, `INSERT INTO domain_events
		(event_id,organization_id,aggregate_type,aggregate_id,aggregate_version,event_type,actor_kind,actor_id,principal_id,command_id,request_id,occurred_at,payload)
		VALUES ($1,$2,'project',$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)`,
		eventID, orgID, projectID, version, eventType, actor.Kind, actor.ID, actor.PrincipalID, commandID, rid, occurredAt, encoded)
	if err != nil {
		return err
	}
	if failAfterEvent {
		return ErrInjectedCrash
	}
	return nil
}

func serviceUnavailable(rid string) generated.Response {
	return problem(http.StatusServiceUnavailable, "service_unavailable", "Service unavailable", "The transaction did not commit; retry with the same key.", rid)
}
