package app

import (
	"context"
	"database/sql"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"sort"
	"strconv"
	"strings"
	"time"
	"unicode/utf8"

	"github.com/agent-team-project/kensho-projects/projects/workplane/internal/api/generated"
	"github.com/lib/pq"
)

type Deliverable struct {
	ID                 string   `json:"id"`
	OrganizationID     string   `json:"organization_id"`
	ProjectID          string   `json:"project_id"`
	Title              string   `json:"title"`
	Description        string   `json:"description"`
	Required           bool     `json:"required"`
	Weight             int64    `json:"weight"`
	State              string   `json:"state"`
	AcceptanceCriteria []string `json:"acceptance_criteria"`
	Version            int64    `json:"version"`
	CreatedBy          string   `json:"created_by"`
	CreatedAt          string   `json:"created_at"`
	UpdatedAt          string   `json:"updated_at"`
}

type Forecast struct {
	ID             string   `json:"id"`
	OrganizationID string   `json:"organization_id"`
	ProjectID      string   `json:"project_id"`
	DeliverableID  *string  `json:"deliverable_id"`
	Scope          string   `json:"scope"`
	P50At          string   `json:"p50_at"`
	P90At          string   `json:"p90_at"`
	ReviewAfter    string   `json:"review_after"`
	Basis          string   `json:"basis"`
	Assumptions    []string `json:"assumptions"`
	ReasonCodes    []string `json:"reason_codes"`
	Impact         string   `json:"impact"`
	SupersedesID   *string  `json:"supersedes_id"`
	CreatedBy      string   `json:"created_by"`
	CreatedAt      string   `json:"created_at"`
}

type ForecastView struct {
	Forecast
	Current       bool `json:"current"`
	Stale         bool `json:"stale"`
	AttentionOnly bool `json:"attention_only"`
}

type ForecastHead struct {
	ProjectID     string  `json:"project_id"`
	DeliverableID *string `json:"deliverable_id"`
	ForecastID    string  `json:"forecast_id"`
}

type ForecastSupersession struct {
	ProjectID     string  `json:"project_id"`
	DeliverableID *string `json:"deliverable_id"`
	SupersededID  string  `json:"superseded_id"`
	SupersedingID string  `json:"superseding_id"`
}

type Target struct {
	ID             string  `json:"id"`
	OrganizationID string  `json:"organization_id"`
	ProjectID      string  `json:"project_id"`
	TargetAt       string  `json:"target_at"`
	Reason         string  `json:"reason"`
	SupersedesID   *string `json:"supersedes_id"`
	CreatedBy      string  `json:"created_by"`
	CreatedAt      string  `json:"created_at"`
}

type TargetView struct {
	Target
	Current       bool `json:"current"`
	Missed        bool `json:"missed"`
	AttentionOnly bool `json:"attention_only"`
}

type TargetHead struct {
	ProjectID string `json:"project_id"`
	TargetID  string `json:"target_id"`
}

type Deadline struct {
	ID             string  `json:"id"`
	OrganizationID string  `json:"organization_id"`
	ProjectID      string  `json:"project_id"`
	DeadlineAt     string  `json:"deadline_at"`
	Source         string  `json:"source"`
	Description    string  `json:"description"`
	SupersedesID   *string `json:"supersedes_id"`
	CreatedBy      string  `json:"created_by"`
	CreatedAt      string  `json:"created_at"`
}

type DeadlineView struct {
	Deadline
	Current       bool `json:"current"`
	Passed        bool `json:"passed"`
	AttentionOnly bool `json:"attention_only"`
}

type DeadlineHead struct {
	ProjectID  string `json:"project_id"`
	DeadlineID string `json:"deadline_id"`
}

type PromotionResult struct {
	Project      Project       `json:"project"`
	Decision     Decision      `json:"decision"`
	Deliverables []Deliverable `json:"deliverables"`
}

type ProjectTransition struct {
	Project Project `json:"project"`
	Reason  string  `json:"reason"`
}

type PromotionEvent struct {
	Project             Project `json:"project"`
	ResidualUncertainty string  `json:"residual_uncertainty"`
	PriorityRationale   string  `json:"priority_rationale"`
}

type mutationOutcome struct {
	Status   int
	Body     any
	Version  int64
	EventIDs []string
}

type projectMutation func(*sql.Tx, Project, Actor, string, time.Time) (mutationOutcome, *generated.Response, error)

func (service *Service) prepareProjectMutation(ctx context.Context, request generated.Request, actions []string, projectID string) (Actor, string, int64, generated.Response, bool) {
	actor, authResponse, ok := service.authenticate(ctx, request, actions[0], projectID)
	if !ok {
		return Actor{}, "", 0, authResponse, false
	}
	rid := authResponse.Headers.XRequestID
	if denied, ok := service.requireHumanMutation(request, actor, rid); !ok {
		return Actor{}, "", 0, denied, false
	}
	if !uuidPattern.MatchString(projectID) {
		return Actor{}, "", 0, problem(http.StatusNotFound, "not_found", "Resource not found", "The requested resource is not available.", rid), false
	}
	if utf8.RuneCountInString(request.IdempotencyKey) < generated.IdempotencyKeyMinLength ||
		utf8.RuneCountInString(request.IdempotencyKey) > generated.IdempotencyKeyMaxLength {
		return Actor{}, "", 0, problem(http.StatusBadRequest, "invalid_request", "Idempotency key required", "Idempotency-Key must contain 16 to 128 characters.", rid), false
	}
	match := versionETagPattern.FindStringSubmatch(string(request.ExpectedVersion))
	if len(match) != 2 {
		return Actor{}, "", 0, problem(http.StatusPreconditionRequired, "version_conflict", "Expected version required", "If-Match must contain the quoted aggregate version.", rid), false
	}
	expected, _ := strconv.ParseInt(match[1], 10, 64)
	if actor.Kind == "agent" {
		for _, action := range actions[1:] {
			if !actor.Scopes[action] {
				service.audit(ctx, "authorization.denied", rid, &actor, map[string]any{"reason": "agent_action_scope", "action": action})
				return Actor{}, "", 0, problem(http.StatusForbidden, "forbidden", "Action denied", "The delegated token does not include every required action.", rid), false
			}
		}
	}
	if denied, ok := service.authorizeProject(ctx, actor, projectID, actor.OrganizationID, actions[0], rid); !ok {
		return Actor{}, "", 0, denied, false
	}
	return actor, rid, expected, generated.Response{}, true
}

func (service *Service) executeProjectMutation(ctx context.Context, request generated.Request, actor Actor, rid, projectID, operation string,
	expected int64, canonical []byte, mutate projectMutation) generated.Response {
	hash := requestHash(canonical, []byte(request.ExpectedVersion))
	tx, err := service.db.BeginTx(ctx, &sql.TxOptions{Isolation: sql.LevelSerializable})
	if err != nil {
		return serviceUnavailable(rid)
	}
	defer tx.Rollback()
	if _, err := tx.ExecContext(ctx, `SELECT pg_advisory_xact_lock(hashtextextended($1,0))`, actor.OrganizationID+actor.ID+operation+request.IdempotencyKey); err != nil {
		return serviceUnavailable(rid)
	}
	if replay, found, conflict := replayIdempotency(ctx, tx, actor.OrganizationID, actor.ID, operation, request.IdempotencyKey, hash); found {
		if conflict {
			return problem(http.StatusConflict, "idempotency_conflict", "Idempotency key conflict", "The key was already used with a different request.", rid)
		}
		return replay
	}
	project, err := scanProject(tx.QueryRowContext(ctx, selectProject+" WHERE id=$1 AND organization_id=$2 FOR UPDATE", projectID, actor.OrganizationID))
	if err != nil {
		if errors.Is(err, sql.ErrNoRows) {
			return problem(http.StatusNotFound, "not_found", "Resource not found", "The requested resource is not available.", rid)
		}
		return planningMutationFailure(err, rid)
	}
	if project.Version != expected {
		return problem(http.StatusConflict, "version_conflict", "Project version changed", fmt.Sprintf("Expected version %d; current version is %d.", expected, project.Version), rid)
	}
	commandID, err := newUUID()
	if err != nil {
		return planningMutationFailure(err, rid)
	}
	request.HTTPRequest.Header.Set("X-Workplane-Command-ID", commandID)
	outcome, rejected, err := mutate(tx, project, actor, rid, service.now().UTC().Truncate(time.Microsecond))
	if rejected != nil {
		return *rejected
	}
	if err != nil {
		return planningMutationFailure(err, rid)
	}
	etag := fmt.Sprintf(`"%d"`, outcome.Version)
	body := canonicalJSON(outcome.Body)
	if err := storeIdempotency(ctx, tx, actor.OrganizationID, actor.ID, operation, request.IdempotencyKey, hash,
		outcome.Status, body, etag, rid, outcome.EventIDs, service.now().UTC().Truncate(time.Microsecond)); err != nil {
		return planningMutationFailure(err, rid)
	}
	if err := tx.Commit(); err != nil {
		return planningMutationFailure(err, rid)
	}
	return generated.Response{Status: outcome.Status, Headers: generated.ResponseHeaders{ETag: generated.VersionETag(etag), XRequestID: rid}, Body: outcome.Body}
}

func planningMutationFailure(err error, rid string) generated.Response {
	var databaseError *pq.Error
	if errors.As(err, &databaseError) && databaseError.Code == "40001" {
		return problem(http.StatusConflict, "version_conflict", "Concurrent project update", "Another command changed the project; retry from its current version.", rid)
	}
	return serviceUnavailable(rid)
}

func rejected(response generated.Response) *generated.Response { return &response }

func mutableProject(project Project) bool {
	return project.State == "proposed" || project.State == "active" || project.State == "held"
}

func insertPlanningEvent(ctx context.Context, tx *sql.Tx, service *Service, request generated.Request, actor Actor, rid string,
	projectID string, version int64, eventType string, payload any, now time.Time, boundary string) (string, error) {
	eventID, err := newUUID()
	if err != nil {
		return "", err
	}
	commandID := request.HTTPRequest.Header.Get("X-Workplane-Command-ID")
	if commandID == "" {
		commandID, err = newUUID()
		if err != nil {
			return "", err
		}
	}
	if err := insertEvent(ctx, tx, eventID, actor.OrganizationID, projectID, version, eventType, actor, commandID, rid, now, payload, false); err != nil {
		return "", err
	}
	if service.config.FaultInjection {
		fault := request.HTTPRequest.Header.Get("X-Workplane-Fault")
		if fault == boundary || fault == "after-event" {
			return "", ErrInjectedCrash
		}
	}
	return eventID, nil
}

func validLifecycleTransition(mode, state, command string, hasForecast, hasRequiredDeliverable bool) bool {
	switch command {
	case "activate":
		return state == "proposed" && hasForecast && (mode == "exploration" || (mode == "exploitation" && hasRequiredDeliverable))
	case "hold":
		return state == "active"
	case "resume":
		return state == "held"
	case "promote":
		return mode == "exploration" && state == "active" && hasForecast && hasRequiredDeliverable
	default:
		return false
	}
}

func normalizeLifecycleReason(input generated.LifecycleReasonRequest) (generated.LifecycleReasonRequest, bool) {
	input.Reason = strings.TrimSpace(input.Reason)
	return input, validText(input.Reason, generated.LifecycleReasonRequestReasonMinLength, generated.LifecycleReasonRequestReasonMaxLength)
}

func (service *Service) hasCurrentForecast(ctx context.Context, tx *sql.Tx, projectID string) bool {
	var exists bool
	return tx.QueryRowContext(ctx, `SELECT EXISTS (SELECT 1 FROM forecast_heads WHERE project_id=$1 AND deliverable_id IS NULL)`, projectID).Scan(&exists) == nil && exists
}

func (service *Service) requiredDeliverableCount(ctx context.Context, tx *sql.Tx, projectID string) int {
	var count int
	_ = tx.QueryRowContext(ctx, `SELECT count(*) FROM deliverables WHERE project_id=$1 AND required AND state NOT IN ('cancelled','waived')`, projectID).Scan(&count)
	return count
}

func (service *Service) lifecycle(ctx context.Context, request generated.Request, command, action, eventType string) (generated.Response, error) {
	projectID := request.HTTPRequest.PathValue("project_id")
	actions := []string{action}
	if command == "hold" || command == "resume" {
		actions = append(actions, "decision.record")
	}
	actor, rid, expected, response, ok := service.prepareProjectMutation(ctx, request, actions, projectID)
	if !ok {
		return response, nil
	}
	input, err := decodeStrict[generated.LifecycleReasonRequest](request.Body)
	input, valid := normalizeLifecycleReason(input)
	if err != nil || !valid {
		return problem(http.StatusBadRequest, "invalid_request", "Invalid lifecycle reason", "A non-empty lifecycle reason is required.", rid), nil
	}
	result := service.executeProjectMutation(ctx, request, actor, rid, projectID, command+"Project", expected, canonicalJSON(input),
		func(tx *sql.Tx, project Project, actor Actor, rid string, now time.Time) (mutationOutcome, *generated.Response, error) {
			hasForecast := service.hasCurrentForecast(ctx, tx, project.ID)
			hasRequired := service.requiredDeliverableCount(ctx, tx, project.ID) > 0
			if !validLifecycleTransition(project.Mode, project.State, command, hasForecast, hasRequired) {
				return mutationOutcome{}, rejected(problem(http.StatusConflict, "invariant_violation", "Lifecycle transition denied", "The project state, mode, or activation contract does not permit this transition.", rid)), nil
			}
			switch command {
			case "activate", "resume":
				project.State = "active"
			case "hold":
				project.State = "held"
			}
			project.Version++
			if _, err := tx.ExecContext(ctx, `UPDATE projects SET state=$1,version=$2,updated_at=$3 WHERE id=$4`, project.State, project.Version, now, project.ID); err != nil {
				return mutationOutcome{}, nil, err
			}
			if service.config.FaultInjection && request.HTTPRequest.Header.Get("X-Workplane-Fault") == "after-project" {
				return mutationOutcome{}, nil, ErrInjectedCrash
			}
			eventID, err := insertPlanningEvent(ctx, tx, service, request, actor, rid, project.ID, project.Version, eventType,
				ProjectTransition{Project: project, Reason: input.Reason}, now, "after-lifecycle-event")
			if err != nil {
				return mutationOutcome{}, nil, err
			}
			return mutationOutcome{Status: http.StatusOK, Body: project, Version: project.Version, EventIDs: []string{eventID}}, nil, nil
		})
	return result, nil
}

func (service *Service) ActivateProject(ctx context.Context, request generated.Request) (generated.Response, error) {
	return service.lifecycle(ctx, request, "activate", "project.activate", "project.activated")
}

func (service *Service) HoldProject(ctx context.Context, request generated.Request) (generated.Response, error) {
	return service.lifecycle(ctx, request, "hold", "project.hold", "project.held")
}

func (service *Service) ResumeProject(ctx context.Context, request generated.Request) (generated.Response, error) {
	return service.lifecycle(ctx, request, "resume", "project.resume", "project.resumed")
}

func normalizeDeliverableInput(input generated.DeliverableInput) (generated.DeliverableInput, bool) {
	input.Title = strings.TrimSpace(input.Title)
	input.Description = strings.TrimSpace(input.Description)
	criteria, ok := normalizeStrings(input.AcceptanceCriteria,
		generated.DeliverableInputAcceptanceCriteriaMinItems,
		generated.DeliverableInputAcceptanceCriteriaMaxItems,
		generated.DeliverableInputAcceptanceCriteriaItemMinLength,
		generated.DeliverableInputAcceptanceCriteriaItemMaxLength)
	if !ok || !validText(input.Title, generated.DeliverableInputTitleMinLength, generated.DeliverableInputTitleMaxLength) ||
		!validText(input.Description, generated.DeliverableInputDescriptionMinLength, generated.DeliverableInputDescriptionMaxLength) ||
		input.Weight < 1 || input.Weight > 1000 || (input.State != "draft" && input.State != "ready") {
		return generated.DeliverableInput{}, false
	}
	input.AcceptanceCriteria = criteria
	return input, true
}

func deliverableFromInput(id, orgID, projectID, actorID string, input generated.DeliverableInput, version int64, created, updated time.Time) Deliverable {
	return Deliverable{ID: id, OrganizationID: orgID, ProjectID: projectID, Title: input.Title, Description: input.Description,
		Required: input.Required, Weight: input.Weight, State: input.State, AcceptanceCriteria: input.AcceptanceCriteria,
		Version: version, CreatedBy: actorID, CreatedAt: created.Format(timeFormat), UpdatedAt: updated.Format(timeFormat)}
}

func insertDeliverable(ctx context.Context, tx *sql.Tx, item Deliverable) error {
	_, err := tx.ExecContext(ctx, `INSERT INTO deliverables
		(id,organization_id,project_id,title,description,required,weight,state,acceptance_criteria,version,created_by,created_at,updated_at)
		VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)`, item.ID, item.OrganizationID, item.ProjectID,
		item.Title, item.Description, item.Required, item.Weight, item.State, canonicalJSON(item.AcceptanceCriteria), item.Version,
		item.CreatedBy, item.CreatedAt, item.UpdatedAt)
	return err
}

func scanDeliverable(row interface{ Scan(...any) error }) (Deliverable, error) {
	var item Deliverable
	var criteria []byte
	var created, updated time.Time
	err := row.Scan(&item.ID, &item.OrganizationID, &item.ProjectID, &item.Title, &item.Description, &item.Required,
		&item.Weight, &item.State, &criteria, &item.Version, &item.CreatedBy, &created, &updated)
	if err != nil {
		return Deliverable{}, err
	}
	if err := json.Unmarshal(criteria, &item.AcceptanceCriteria); err != nil {
		return Deliverable{}, err
	}
	item.CreatedAt, item.UpdatedAt = created.UTC().Format(timeFormat), updated.UTC().Format(timeFormat)
	return item, nil
}

const selectDeliverable = `SELECT id,organization_id,project_id,title,description,required,weight,state,acceptance_criteria,version,created_by,created_at,updated_at FROM deliverables`

func (service *Service) CreateDeliverable(ctx context.Context, request generated.Request) (generated.Response, error) {
	projectID := request.HTTPRequest.PathValue("project_id")
	actor, rid, expected, response, ok := service.prepareProjectMutation(ctx, request, []string{"deliverable.edit"}, projectID)
	if !ok {
		return response, nil
	}
	input, err := decodeStrict[generated.DeliverableInput](request.Body)
	input, valid := normalizeDeliverableInput(input)
	if err != nil || !valid {
		return problem(http.StatusBadRequest, "invalid_request", "Invalid deliverable", "The deliverable does not satisfy the required, weight, state, and acceptance-criteria contract.", rid), nil
	}
	result := service.executeProjectMutation(ctx, request, actor, rid, projectID, "createDeliverable", expected, canonicalJSON(input),
		func(tx *sql.Tx, project Project, actor Actor, rid string, now time.Time) (mutationOutcome, *generated.Response, error) {
			if !mutableProject(project) || project.Mode != "exploitation" {
				return mutationOutcome{}, rejected(problem(http.StatusConflict, "invariant_violation", "Deliverable not allowed", "Deliverables can be created only for a mutable exploitation project.", rid)), nil
			}
			id, err := newUUID()
			if err != nil {
				return mutationOutcome{}, nil, err
			}
			item := deliverableFromInput(id, actor.OrganizationID, project.ID, actor.ID, input, 1, now, now)
			if err := insertDeliverable(ctx, tx, item); err != nil {
				return mutationOutcome{}, nil, err
			}
			if service.config.FaultInjection && request.HTTPRequest.Header.Get("X-Workplane-Fault") == "after-deliverable" {
				return mutationOutcome{}, nil, ErrInjectedCrash
			}
			project.Version++
			if _, err := tx.ExecContext(ctx, `UPDATE projects SET version=$1,updated_at=$2 WHERE id=$3`, project.Version, now, project.ID); err != nil {
				return mutationOutcome{}, nil, err
			}
			eventID, err := insertPlanningEvent(ctx, tx, service, request, actor, rid, project.ID, project.Version, "deliverable.created", item, now, "after-deliverable-event")
			if err != nil {
				return mutationOutcome{}, nil, err
			}
			return mutationOutcome{Status: http.StatusCreated, Body: item, Version: project.Version, EventIDs: []string{eventID}}, nil, nil
		})
	return result, nil
}

func (service *Service) deliverableProjectID(ctx context.Context, id string) (string, bool) {
	if !uuidPattern.MatchString(id) {
		return "", false
	}
	var projectID string
	if err := service.db.QueryRowContext(ctx, `SELECT project_id FROM deliverables WHERE id=$1`, id).Scan(&projectID); err != nil {
		return "", false
	}
	return projectID, true
}

func (service *Service) GetDeliverable(ctx context.Context, request generated.Request) (generated.Response, error) {
	id := request.HTTPRequest.PathValue("id")
	projectID, found := service.deliverableProjectID(ctx, id)
	actor, authResponse, ok := service.authenticate(ctx, request, "deliverable.read", projectID)
	if !ok {
		return authResponse, nil
	}
	rid := authResponse.Headers.XRequestID
	if len(strings.TrimSpace(string(request.Body))) > 0 {
		return problem(http.StatusBadRequest, "invalid_request", "Invalid request", "GET requests cannot contain a body.", rid), nil
	}
	if !found {
		return problem(http.StatusNotFound, "not_found", "Resource not found", "The requested resource is not available.", rid), nil
	}
	item, err := scanDeliverable(service.db.QueryRowContext(ctx, selectDeliverable+` WHERE id=$1 AND organization_id=$2`, id, actor.OrganizationID))
	if err != nil {
		return problem(http.StatusNotFound, "not_found", "Resource not found", "The requested resource is not available.", rid), nil
	}
	if denied, ok := service.authorizeProject(ctx, actor, item.ProjectID, item.OrganizationID, "deliverable.read", rid); !ok {
		return denied, nil
	}
	return generated.Response{Status: http.StatusOK, Headers: generated.ResponseHeaders{XRequestID: rid}, Body: item}, nil
}

func (service *Service) ListDeliverables(ctx context.Context, request generated.Request) (generated.Response, error) {
	projectID := request.HTTPRequest.PathValue("project_id")
	actor, authResponse, ok := service.authenticate(ctx, request, "deliverable.read", projectID)
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
	if denied, ok := service.authorizeProject(ctx, actor, projectID, actor.OrganizationID, "deliverable.read", rid); !ok {
		return denied, nil
	}
	rows, err := service.db.QueryContext(ctx, selectDeliverable+` WHERE project_id=$1 AND organization_id=$2 ORDER BY created_at,id`, projectID, actor.OrganizationID)
	if err != nil {
		return serviceUnavailable(rid), nil
	}
	defer rows.Close()
	items := make([]Deliverable, 0)
	for rows.Next() {
		item, err := scanDeliverable(rows)
		if err != nil {
			return serviceUnavailable(rid), nil
		}
		items = append(items, item)
	}
	if err := rows.Err(); err != nil {
		return serviceUnavailable(rid), nil
	}
	return generated.Response{Status: http.StatusOK, Headers: generated.ResponseHeaders{XRequestID: rid}, Body: items}, nil
}

func (service *Service) ReviseDeliverable(ctx context.Context, request generated.Request) (generated.Response, error) {
	id := request.HTTPRequest.PathValue("id")
	projectID, found := service.deliverableProjectID(ctx, id)
	if !found {
		projectID = "00000000-0000-4000-8000-000000000000"
	}
	actor, rid, expected, response, ok := service.prepareProjectMutation(ctx, request, []string{"deliverable.edit"}, projectID)
	if !ok {
		return response, nil
	}
	input, err := decodeStrict[generated.DeliverableInput](request.Body)
	input, valid := normalizeDeliverableInput(input)
	if err != nil || !valid {
		return problem(http.StatusBadRequest, "invalid_request", "Invalid deliverable", "The deliverable revision violates the public contract.", rid), nil
	}
	result := service.executeProjectMutation(ctx, request, actor, rid, projectID, "reviseDeliverable", expected, canonicalJSON(input),
		func(tx *sql.Tx, project Project, actor Actor, rid string, now time.Time) (mutationOutcome, *generated.Response, error) {
			if !mutableProject(project) || project.Mode != "exploitation" {
				return mutationOutcome{}, rejected(problem(http.StatusConflict, "invariant_violation", "Deliverable not editable", "The owning project is not a mutable exploitation project.", rid)), nil
			}
			item, err := scanDeliverable(tx.QueryRowContext(ctx, selectDeliverable+` WHERE id=$1 AND project_id=$2 FOR UPDATE`, id, project.ID))
			if err != nil {
				return mutationOutcome{}, rejected(problem(http.StatusNotFound, "not_found", "Resource not found", "The requested resource is not available.", rid)), nil
			}
			if item.State == "accepted" || item.State == "waived" || item.State == "cancelled" {
				return mutationOutcome{}, rejected(problem(http.StatusConflict, "invariant_violation", "Deliverable is immutable", "Accepted, waived, and cancelled deliverables cannot be edited.", rid)), nil
			}
			item.Title, item.Description, item.Required, item.Weight, item.State = input.Title, input.Description, input.Required, input.Weight, input.State
			item.AcceptanceCriteria, item.Version, item.UpdatedAt = input.AcceptanceCriteria, item.Version+1, now.Format(timeFormat)
			if _, err := tx.ExecContext(ctx, `UPDATE deliverables SET title=$1,description=$2,required=$3,weight=$4,state=$5,
				acceptance_criteria=$6,version=$7,updated_at=$8 WHERE id=$9`, item.Title, item.Description, item.Required,
				item.Weight, item.State, canonicalJSON(item.AcceptanceCriteria), item.Version, now, item.ID); err != nil {
				return mutationOutcome{}, nil, err
			}
			if service.config.FaultInjection && request.HTTPRequest.Header.Get("X-Workplane-Fault") == "after-deliverable" {
				return mutationOutcome{}, nil, ErrInjectedCrash
			}
			project.Version++
			if _, err := tx.ExecContext(ctx, `UPDATE projects SET version=$1,updated_at=$2 WHERE id=$3`, project.Version, now, project.ID); err != nil {
				return mutationOutcome{}, nil, err
			}
			eventID, err := insertPlanningEvent(ctx, tx, service, request, actor, rid, project.ID, project.Version, "deliverable.revised", item, now, "after-deliverable-event")
			if err != nil {
				return mutationOutcome{}, nil, err
			}
			return mutationOutcome{Status: http.StatusOK, Body: item, Version: project.Version, EventIDs: []string{eventID}}, nil, nil
		})
	return result, nil
}

func normalizePromotionDecision(input generated.PromotionDecision) (generated.PromotionDecision, bool) {
	input.Question, input.Choice, input.Rationale = strings.TrimSpace(input.Question), strings.TrimSpace(input.Choice), strings.TrimSpace(input.Rationale)
	if !validText(input.Question, 1, 2000) || !validText(input.Choice, 1, 2000) || !validText(input.Rationale, 1, 4000) {
		return generated.PromotionDecision{}, false
	}
	var valid bool
	if input.Alternatives, valid = normalizeStrings(input.Alternatives, 0, 32, 0, 1000); !valid {
		return generated.PromotionDecision{}, false
	}
	if input.Evidence, valid = normalizeStrings(input.Evidence, 0, 64, 0, 2000); !valid {
		return generated.PromotionDecision{}, false
	}
	if input.Consequences, valid = normalizeStrings(input.Consequences, 0, 32, 0, 2000); !valid {
		return generated.PromotionDecision{}, false
	}
	return input, true
}

func (service *Service) PromoteProject(ctx context.Context, request generated.Request) (generated.Response, error) {
	projectID := request.HTTPRequest.PathValue("project_id")
	actor, rid, expected, response, ok := service.prepareProjectMutation(ctx, request, []string{"project.promote", "decision.record"}, projectID)
	if !ok {
		return response, nil
	}
	input, err := decodeStrict[generated.PromoteProjectRequest](request.Body)
	input.ResidualUncertainty, input.PriorityRationale = strings.TrimSpace(input.ResidualUncertainty), strings.TrimSpace(input.PriorityRationale)
	input.Decision, ok = normalizePromotionDecision(input.Decision)
	valid := err == nil && ok && validText(input.ResidualUncertainty, 1, 4000) && validText(input.PriorityRationale, 1, 4000) &&
		len(input.Deliverables) >= 1 && len(input.Deliverables) <= 32
	requiredCount := 0
	if valid {
		for index := range input.Deliverables {
			input.Deliverables[index], ok = normalizeDeliverableInput(input.Deliverables[index])
			if !ok {
				valid = false
				break
			}
			if input.Deliverables[index].Required {
				requiredCount++
			}
		}
	}
	if !valid || requiredCount == 0 {
		return problem(http.StatusBadRequest, "invalid_request", "Invalid promotion contract", "Promotion requires a complete decision, residual uncertainty, priority rationale, and a required deliverable with criteria.", rid), nil
	}
	result := service.executeProjectMutation(ctx, request, actor, rid, projectID, "promoteProject", expected, canonicalJSON(input),
		func(tx *sql.Tx, project Project, actor Actor, rid string, now time.Time) (mutationOutcome, *generated.Response, error) {
			if !validLifecycleTransition(project.Mode, project.State, "promote", service.hasCurrentForecast(ctx, tx, project.ID), true) {
				return mutationOutcome{}, rejected(problem(http.StatusConflict, "invariant_violation", "Promotion denied", "Only an active forecasted exploration project can be promoted.", rid)), nil
			}
			decisionID, err := newUUID()
			if err != nil {
				return mutationOutcome{}, nil, err
			}
			decision := Decision{ID: decisionID, ProjectID: project.ID, ActorID: actor.ID, ActorKind: actor.Kind, PrincipalID: actor.PrincipalID,
				RecordedAt: now.Format(timeFormat), Kind: "promote", Question: input.Decision.Question, Choice: input.Decision.Choice,
				Alternatives: input.Decision.Alternatives, Rationale: input.Decision.Rationale, Evidence: input.Decision.Evidence,
				Consequences: input.Decision.Consequences}
			if _, err := tx.ExecContext(ctx, `INSERT INTO decisions
				(id,organization_id,project_id,kind,question,choice,alternatives,rationale,evidence,consequences,actor_id,recorded_at)
				VALUES ($1,$2,$3,'promote',$4,$5,$6,$7,$8,$9,$10,$11)`, decision.ID, actor.OrganizationID, project.ID,
				decision.Question, decision.Choice, canonicalJSON(decision.Alternatives), decision.Rationale, canonicalJSON(decision.Evidence),
				canonicalJSON(decision.Consequences), actor.ID, now); err != nil {
				return mutationOutcome{}, nil, err
			}
			if service.config.FaultInjection && request.HTTPRequest.Header.Get("X-Workplane-Fault") == "after-decision" {
				return mutationOutcome{}, nil, ErrInjectedCrash
			}
			version := project.Version + 1
			decisionEvent, err := insertPlanningEvent(ctx, tx, service, request, actor, rid, project.ID, version, "decision.recorded", decision, now, "after-decision-event")
			if err != nil {
				return mutationOutcome{}, nil, err
			}
			eventIDs := []string{decisionEvent}
			deliverables := make([]Deliverable, 0, len(input.Deliverables))
			for _, deliverableInput := range input.Deliverables {
				id, err := newUUID()
				if err != nil {
					return mutationOutcome{}, nil, err
				}
				item := deliverableFromInput(id, actor.OrganizationID, project.ID, actor.ID, deliverableInput, 1, now, now)
				if err := insertDeliverable(ctx, tx, item); err != nil {
					return mutationOutcome{}, nil, err
				}
				if service.config.FaultInjection && request.HTTPRequest.Header.Get("X-Workplane-Fault") == "after-deliverable" {
					return mutationOutcome{}, nil, ErrInjectedCrash
				}
				version++
				eventID, err := insertPlanningEvent(ctx, tx, service, request, actor, rid, project.ID, version, "deliverable.created", item, now, "after-deliverable-event")
				if err != nil {
					return mutationOutcome{}, nil, err
				}
				eventIDs = append(eventIDs, eventID)
				deliverables = append(deliverables, item)
			}
			project.Mode, project.Version = "exploitation", version+1
			if _, err := tx.ExecContext(ctx, `UPDATE projects SET mode='exploitation',version=$1,updated_at=$2 WHERE id=$3`, project.Version, now, project.ID); err != nil {
				return mutationOutcome{}, nil, err
			}
			if service.config.FaultInjection && request.HTTPRequest.Header.Get("X-Workplane-Fault") == "after-project" {
				return mutationOutcome{}, nil, ErrInjectedCrash
			}
			promotedEvent, err := insertPlanningEvent(ctx, tx, service, request, actor, rid, project.ID, project.Version, "project.promoted",
				PromotionEvent{Project: project, ResidualUncertainty: input.ResidualUncertainty, PriorityRationale: input.PriorityRationale}, now, "after-promotion-event")
			if err != nil {
				return mutationOutcome{}, nil, err
			}
			eventIDs = append(eventIDs, promotedEvent)
			body := PromotionResult{Project: project, Decision: decision, Deliverables: deliverables}
			return mutationOutcome{Status: http.StatusOK, Body: body, Version: project.Version, EventIDs: eventIDs}, nil, nil
		})
	return result, nil
}

var forecastReasonCodes = map[string]bool{
	"scope-change": true, "new-evidence": true, "dependency-change": true, "capacity-change": true,
	"quality-finding": true, "incident": true, "estimate-correction": true, "hold-change": true, "other": true,
}

func normalizeForecastInput(input generated.ForecastInput) (generated.ForecastInput, time.Time, time.Time, time.Time, bool) {
	p50, err50 := time.Parse(time.RFC3339Nano, input.P50At)
	p90, err90 := time.Parse(time.RFC3339Nano, input.P90At)
	review, errReview := time.Parse(time.RFC3339Nano, input.ReviewAfter)
	input.Basis, input.Impact = strings.TrimSpace(input.Basis), strings.TrimSpace(input.Impact)
	assumptions, assumptionsOK := normalizeStrings(input.Assumptions,
		generated.ForecastInputAssumptionsMinItems, generated.ForecastInputAssumptionsMaxItems,
		generated.ForecastInputAssumptionsItemMinLength, generated.ForecastInputAssumptionsItemMaxLength)
	seen := make(map[string]bool)
	reasonsOK := len(input.ReasonCodes) >= generated.ForecastInputReasonCodesMinItems && len(input.ReasonCodes) <= generated.ForecastInputReasonCodesMaxItems
	for index, reason := range input.ReasonCodes {
		reason = strings.TrimSpace(reason)
		input.ReasonCodes[index] = reason
		if !forecastReasonCodes[reason] || seen[reason] {
			reasonsOK = false
		}
		seen[reason] = true
	}
	valid := err50 == nil && err90 == nil && errReview == nil && !p50.After(p90) &&
		validText(input.Basis, generated.ForecastInputBasisMinLength, generated.ForecastInputBasisMaxLength) &&
		validText(input.Impact, generated.ForecastInputImpactMinLength, generated.ForecastInputImpactMaxLength) && assumptionsOK && reasonsOK
	if !valid {
		return generated.ForecastInput{}, time.Time{}, time.Time{}, time.Time{}, false
	}
	input.Assumptions = assumptions
	p50, p90, review = p50.UTC().Truncate(time.Microsecond), p90.UTC().Truncate(time.Microsecond), review.UTC().Truncate(time.Microsecond)
	input.P50At, input.P90At, input.ReviewAfter = p50.Format(timeFormat), p90.Format(timeFormat), review.Format(timeFormat)
	return input, p50, p90, review, true
}

func scanForecast(row interface{ Scan(...any) error }) (Forecast, error) {
	var item Forecast
	var deliverableID, supersedesID sql.NullString
	var p50, p90, review, created time.Time
	var assumptions []byte
	err := row.Scan(&item.ID, &item.OrganizationID, &item.ProjectID, &deliverableID, &p50, &p90, &review,
		&item.Basis, &assumptions, pq.Array(&item.ReasonCodes), &item.Impact, &supersedesID, &item.CreatedBy, &created)
	if err != nil {
		return Forecast{}, err
	}
	if deliverableID.Valid {
		item.DeliverableID = &deliverableID.String
		item.Scope = "deliverable"
	} else {
		item.Scope = "project"
	}
	if supersedesID.Valid {
		item.SupersedesID = &supersedesID.String
	}
	if err := json.Unmarshal(assumptions, &item.Assumptions); err != nil {
		return Forecast{}, err
	}
	item.P50At, item.P90At, item.ReviewAfter, item.CreatedAt = p50.UTC().Format(timeFormat), p90.UTC().Format(timeFormat), review.UTC().Format(timeFormat), created.UTC().Format(timeFormat)
	return item, nil
}

const selectForecast = `SELECT id,organization_id,project_id,deliverable_id,p50_at,p90_at,review_after,basis,assumptions,reason_codes,impact,supersedes_id,created_by,created_at FROM forecasts`

func forecastView(item Forecast, current bool, now time.Time) ForecastView {
	review, _ := time.Parse(timeFormat, item.ReviewAfter)
	return ForecastView{Forecast: item, Current: current, Stale: current && !now.Before(review), AttentionOnly: true}
}

func (service *Service) reforecast(ctx context.Context, request generated.Request, projectID string, deliverableID *string, action, operation string) (generated.Response, error) {
	actor, rid, expected, response, ok := service.prepareProjectMutation(ctx, request, []string{action}, projectID)
	if !ok {
		return response, nil
	}
	input, _, _, _, valid := normalizeForecastInput(func() generated.ForecastInput {
		value, _ := decodeStrict[generated.ForecastInput](request.Body)
		return value
	}())
	if _, err := decodeStrict[generated.ForecastInput](request.Body); err != nil {
		valid = false
	}
	if !valid {
		return problem(http.StatusBadRequest, "invalid_request", "Invalid forecast", "Forecasts require absolute P50/P90/review instants, P50 <= P90, basis, assumptions, unique reasons, and impact.", rid), nil
	}
	canonical := canonicalJSON(input)
	result := service.executeProjectMutation(ctx, request, actor, rid, projectID, operation, expected, canonical,
		func(tx *sql.Tx, project Project, actor Actor, rid string, now time.Time) (mutationOutcome, *generated.Response, error) {
			if !mutableProject(project) {
				return mutationOutcome{}, rejected(problem(http.StatusConflict, "invariant_violation", "Forecast not allowed", "Terminal projects cannot be reforecast.", rid)), nil
			}
			if deliverableID != nil {
				var state string
				if err := tx.QueryRowContext(ctx, `SELECT state::text FROM deliverables WHERE id=$1 AND project_id=$2`, *deliverableID, project.ID).Scan(&state); err != nil {
					return mutationOutcome{}, rejected(problem(http.StatusNotFound, "not_found", "Resource not found", "The requested resource is not available.", rid)), nil
				}
				if state == "accepted" || state == "waived" || state == "cancelled" {
					return mutationOutcome{}, rejected(problem(http.StatusConflict, "invariant_violation", "Forecast not allowed", "Immutable deliverables cannot be reforecast.", rid)), nil
				}
			}
			var currentID sql.NullString
			err := tx.QueryRowContext(ctx, `SELECT forecast_id FROM forecast_heads WHERE project_id=$1 AND deliverable_id IS NOT DISTINCT FROM $2::uuid FOR UPDATE`, project.ID, deliverableID).Scan(&currentID)
			if err != nil && err != sql.ErrNoRows {
				return mutationOutcome{}, nil, err
			}
			id, err := newUUID()
			if err != nil {
				return mutationOutcome{}, nil, err
			}
			var supersedes *string
			if currentID.Valid {
				supersedes = &currentID.String
			}
			item := Forecast{ID: id, OrganizationID: actor.OrganizationID, ProjectID: project.ID, DeliverableID: deliverableID,
				Scope: "project", P50At: input.P50At, P90At: input.P90At, ReviewAfter: input.ReviewAfter, Basis: input.Basis,
				Assumptions: input.Assumptions, ReasonCodes: input.ReasonCodes, Impact: input.Impact, SupersedesID: supersedes,
				CreatedBy: actor.ID, CreatedAt: now.Format(timeFormat)}
			if deliverableID != nil {
				item.Scope = "deliverable"
			}
			if _, err := tx.ExecContext(ctx, `INSERT INTO forecasts
				(id,organization_id,project_id,deliverable_id,p50_at,p90_at,review_after,basis,assumptions,reason_codes,impact,supersedes_id,created_by,created_at)
				VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)`, item.ID, item.OrganizationID, item.ProjectID,
				item.DeliverableID, item.P50At, item.P90At, item.ReviewAfter, item.Basis, canonicalJSON(item.Assumptions), pq.Array(item.ReasonCodes),
				item.Impact, item.SupersedesID, item.CreatedBy, now); err != nil {
				return mutationOutcome{}, nil, err
			}
			if _, err := tx.ExecContext(ctx, `INSERT INTO forecast_heads (project_id,deliverable_id,forecast_id,updated_at)
				VALUES ($1,$2,$3,$4) ON CONFLICT (project_id,scope_key) DO UPDATE SET forecast_id=EXCLUDED.forecast_id,updated_at=EXCLUDED.updated_at`,
				project.ID, deliverableID, item.ID, now); err != nil {
				return mutationOutcome{}, nil, err
			}
			if service.config.FaultInjection && request.HTTPRequest.Header.Get("X-Workplane-Fault") == "after-forecast" {
				return mutationOutcome{}, nil, ErrInjectedCrash
			}
			version := project.Version
			eventIDs := make([]string, 0, 2)
			if supersedes != nil {
				version++
				eventID, err := insertPlanningEvent(ctx, tx, service, request, actor, rid, project.ID, version, "forecast.superseded",
					ForecastSupersession{ProjectID: project.ID, DeliverableID: deliverableID, SupersededID: *supersedes, SupersedingID: item.ID}, now, "after-forecast-superseded-event")
				if err != nil {
					return mutationOutcome{}, nil, err
				}
				eventIDs = append(eventIDs, eventID)
			}
			version++
			createdEvent, err := insertPlanningEvent(ctx, tx, service, request, actor, rid, project.ID, version, "forecast.created", item, now, "after-forecast-created-event")
			if err != nil {
				return mutationOutcome{}, nil, err
			}
			eventIDs = append(eventIDs, createdEvent)
			if _, err := tx.ExecContext(ctx, `UPDATE projects SET version=$1,updated_at=$2 WHERE id=$3`, version, now, project.ID); err != nil {
				return mutationOutcome{}, nil, err
			}
			return mutationOutcome{Status: http.StatusCreated, Body: forecastView(item, true, service.now()), Version: version, EventIDs: eventIDs}, nil, nil
		})
	return result, nil
}

func (service *Service) ReforecastProject(ctx context.Context, request generated.Request) (generated.Response, error) {
	return service.reforecast(ctx, request, request.HTTPRequest.PathValue("project_id"), nil, "project.reforecast", "reforecastProject")
}

func (service *Service) ReforecastDeliverable(ctx context.Context, request generated.Request) (generated.Response, error) {
	id := request.HTTPRequest.PathValue("deliverable_id")
	projectID, found := service.deliverableProjectID(ctx, id)
	if !found {
		projectID = "00000000-0000-4000-8000-000000000000"
	}
	return service.reforecast(ctx, request, projectID, &id, "deliverable.reforecast", "reforecastDeliverable")
}

func (service *Service) listForecasts(ctx context.Context, request generated.Request, projectID string, deliverableID *string, action string) (generated.Response, error) {
	actor, authResponse, ok := service.authenticate(ctx, request, action, projectID)
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
	if denied, ok := service.authorizeProject(ctx, actor, projectID, actor.OrganizationID, action, rid); !ok {
		return denied, nil
	}
	rows, err := service.db.QueryContext(ctx, `SELECT forecast.id,forecast.organization_id,forecast.project_id,forecast.deliverable_id,
		forecast.p50_at,forecast.p90_at,forecast.review_after,forecast.basis,forecast.assumptions,forecast.reason_codes,
		forecast.impact,forecast.supersedes_id,forecast.created_by,forecast.created_at,
		(head.forecast_id=forecast.id) AS current
		FROM forecasts forecast
		LEFT JOIN forecast_heads head ON head.project_id=forecast.project_id
			AND head.deliverable_id IS NOT DISTINCT FROM forecast.deliverable_id
		WHERE forecast.project_id=$1 AND forecast.organization_id=$2 AND forecast.deliverable_id IS NOT DISTINCT FROM $3::uuid
		ORDER BY forecast.created_at,forecast.id`, projectID, actor.OrganizationID, deliverableID)
	if err != nil {
		return serviceUnavailable(rid), nil
	}
	defer rows.Close()
	items := make([]ForecastView, 0)
	for rows.Next() {
		var item Forecast
		var deliverable, supersedes sql.NullString
		var p50, p90, review, created time.Time
		var assumptions []byte
		var current bool
		if err := rows.Scan(&item.ID, &item.OrganizationID, &item.ProjectID, &deliverable, &p50, &p90, &review, &item.Basis,
			&assumptions, pq.Array(&item.ReasonCodes), &item.Impact, &supersedes, &item.CreatedBy, &created, &current); err != nil {
			return serviceUnavailable(rid), nil
		}
		if deliverable.Valid {
			item.DeliverableID, item.Scope = &deliverable.String, "deliverable"
		} else {
			item.Scope = "project"
		}
		if supersedes.Valid {
			item.SupersedesID = &supersedes.String
		}
		if err := json.Unmarshal(assumptions, &item.Assumptions); err != nil {
			return serviceUnavailable(rid), nil
		}
		item.P50At, item.P90At, item.ReviewAfter, item.CreatedAt = p50.UTC().Format(timeFormat), p90.UTC().Format(timeFormat), review.UTC().Format(timeFormat), created.UTC().Format(timeFormat)
		items = append(items, forecastView(item, current, service.now()))
	}
	return generated.Response{Status: http.StatusOK, Headers: generated.ResponseHeaders{XRequestID: rid}, Body: items}, nil
}

func (service *Service) ListProjectForecasts(ctx context.Context, request generated.Request) (generated.Response, error) {
	return service.listForecasts(ctx, request, request.HTTPRequest.PathValue("project_id"), nil, "project.read")
}

func (service *Service) ListDeliverableForecasts(ctx context.Context, request generated.Request) (generated.Response, error) {
	id := request.HTTPRequest.PathValue("deliverable_id")
	projectID, found := service.deliverableProjectID(ctx, id)
	if !found {
		projectID = "00000000-0000-4000-8000-000000000000"
	}
	return service.listForecasts(ctx, request, projectID, &id, "deliverable.read")
}

func (service *Service) SetProjectTarget(ctx context.Context, request generated.Request) (generated.Response, error) {
	projectID := request.HTTPRequest.PathValue("project_id")
	actor, rid, expected, response, ok := service.prepareProjectMutation(ctx, request, []string{"project.target.write"}, projectID)
	if !ok {
		return response, nil
	}
	input, err := decodeStrict[generated.TargetInput](request.Body)
	targetAt, dateErr := time.Parse(time.RFC3339Nano, input.TargetAt)
	input.Reason = strings.TrimSpace(input.Reason)
	if err != nil || dateErr != nil || !validText(input.Reason, generated.TargetInputReasonMinLength, generated.TargetInputReasonMaxLength) {
		return problem(http.StatusBadRequest, "invalid_request", "Invalid target", "An absolute intent target and non-empty reason are required.", rid), nil
	}
	targetAt = targetAt.UTC().Truncate(time.Microsecond)
	input.TargetAt = targetAt.Format(timeFormat)
	result := service.executeProjectMutation(ctx, request, actor, rid, projectID, "setProjectTarget", expected, canonicalJSON(input),
		func(tx *sql.Tx, project Project, actor Actor, rid string, now time.Time) (mutationOutcome, *generated.Response, error) {
			if !mutableProject(project) {
				return mutationOutcome{}, rejected(problem(http.StatusConflict, "invariant_violation", "Target not allowed", "Terminal projects cannot change intent targets.", rid)), nil
			}
			var prior sql.NullString
			err := tx.QueryRowContext(ctx, `SELECT target_id FROM project_target_heads WHERE project_id=$1 FOR UPDATE`, project.ID).Scan(&prior)
			if err != nil && err != sql.ErrNoRows {
				return mutationOutcome{}, nil, err
			}
			id, err := newUUID()
			if err != nil {
				return mutationOutcome{}, nil, err
			}
			var supersedes *string
			if prior.Valid {
				supersedes = &prior.String
			}
			item := Target{ID: id, OrganizationID: actor.OrganizationID, ProjectID: project.ID, TargetAt: input.TargetAt,
				Reason: input.Reason, SupersedesID: supersedes, CreatedBy: actor.ID, CreatedAt: now.Format(timeFormat)}
			if _, err := tx.ExecContext(ctx, `INSERT INTO project_targets
				(id,organization_id,project_id,target_at,reason,supersedes_id,created_by,created_at)
				VALUES ($1,$2,$3,$4,$5,$6,$7,$8)`, item.ID, item.OrganizationID, item.ProjectID, targetAt,
				item.Reason, item.SupersedesID, item.CreatedBy, now); err != nil {
				return mutationOutcome{}, nil, err
			}
			if _, err := tx.ExecContext(ctx, `INSERT INTO project_target_heads (project_id,target_id,updated_at) VALUES ($1,$2,$3)
				ON CONFLICT (project_id) DO UPDATE SET target_id=EXCLUDED.target_id,updated_at=EXCLUDED.updated_at`, project.ID, item.ID, now); err != nil {
				return mutationOutcome{}, nil, err
			}
			if service.config.FaultInjection && request.HTTPRequest.Header.Get("X-Workplane-Fault") == "after-target" {
				return mutationOutcome{}, nil, ErrInjectedCrash
			}
			project.Version++
			if _, err := tx.ExecContext(ctx, `UPDATE projects SET version=$1,updated_at=$2 WHERE id=$3`, project.Version, now, project.ID); err != nil {
				return mutationOutcome{}, nil, err
			}
			eventID, err := insertPlanningEvent(ctx, tx, service, request, actor, rid, project.ID, project.Version, "target.changed", item, now, "after-target-event")
			if err != nil {
				return mutationOutcome{}, nil, err
			}
			view := TargetView{Target: item, Current: true, Missed: !service.now().Before(targetAt), AttentionOnly: true}
			return mutationOutcome{Status: http.StatusCreated, Body: view, Version: project.Version, EventIDs: []string{eventID}}, nil, nil
		})
	return result, nil
}

func (service *Service) SetProjectDeadline(ctx context.Context, request generated.Request) (generated.Response, error) {
	projectID := request.HTTPRequest.PathValue("project_id")
	actor, rid, expected, response, ok := service.prepareProjectMutation(ctx, request, []string{"project.deadline.write"}, projectID)
	if !ok {
		return response, nil
	}
	input, err := decodeStrict[generated.DeadlineInput](request.Body)
	deadlineAt, dateErr := time.Parse(time.RFC3339Nano, input.DeadlineAt)
	input.Description = strings.TrimSpace(input.Description)
	validSource := input.Source == "contract" || input.Source == "launch-window" || input.Source == "demonstration" || input.Source == "regulation" || input.Source == "other"
	if err != nil || dateErr != nil || !validSource || !validText(input.Description, generated.DeadlineInputDescriptionMinLength, generated.DeadlineInputDescriptionMaxLength) {
		return problem(http.StatusBadRequest, "invalid_request", "Invalid deadline", "A typed absolute deadline and description are required.", rid), nil
	}
	deadlineAt = deadlineAt.UTC().Truncate(time.Microsecond)
	input.DeadlineAt = deadlineAt.Format(timeFormat)
	result := service.executeProjectMutation(ctx, request, actor, rid, projectID, "setProjectDeadline", expected, canonicalJSON(input),
		func(tx *sql.Tx, project Project, actor Actor, rid string, now time.Time) (mutationOutcome, *generated.Response, error) {
			if !mutableProject(project) {
				return mutationOutcome{}, rejected(problem(http.StatusConflict, "invariant_violation", "Deadline not allowed", "Terminal projects cannot change deadlines.", rid)), nil
			}
			var prior sql.NullString
			err := tx.QueryRowContext(ctx, `SELECT deadline_id FROM project_deadline_heads WHERE project_id=$1 FOR UPDATE`, project.ID).Scan(&prior)
			if err != nil && err != sql.ErrNoRows {
				return mutationOutcome{}, nil, err
			}
			id, err := newUUID()
			if err != nil {
				return mutationOutcome{}, nil, err
			}
			var supersedes *string
			if prior.Valid {
				supersedes = &prior.String
			}
			item := Deadline{ID: id, OrganizationID: actor.OrganizationID, ProjectID: project.ID, DeadlineAt: input.DeadlineAt,
				Source: input.Source, Description: input.Description, SupersedesID: supersedes, CreatedBy: actor.ID, CreatedAt: now.Format(timeFormat)}
			if _, err := tx.ExecContext(ctx, `INSERT INTO project_deadlines
				(id,organization_id,project_id,deadline_at,source,description,supersedes_id,created_by,created_at)
				VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)`, item.ID, item.OrganizationID, item.ProjectID, deadlineAt,
				item.Source, item.Description, item.SupersedesID, item.CreatedBy, now); err != nil {
				return mutationOutcome{}, nil, err
			}
			if _, err := tx.ExecContext(ctx, `INSERT INTO project_deadline_heads (project_id,deadline_id,updated_at) VALUES ($1,$2,$3)
				ON CONFLICT (project_id) DO UPDATE SET deadline_id=EXCLUDED.deadline_id,updated_at=EXCLUDED.updated_at`, project.ID, item.ID, now); err != nil {
				return mutationOutcome{}, nil, err
			}
			if service.config.FaultInjection && request.HTTPRequest.Header.Get("X-Workplane-Fault") == "after-deadline" {
				return mutationOutcome{}, nil, ErrInjectedCrash
			}
			project.Version++
			if _, err := tx.ExecContext(ctx, `UPDATE projects SET version=$1,updated_at=$2 WHERE id=$3`, project.Version, now, project.ID); err != nil {
				return mutationOutcome{}, nil, err
			}
			eventID, err := insertPlanningEvent(ctx, tx, service, request, actor, rid, project.ID, project.Version, "deadline.changed", item, now, "after-deadline-event")
			if err != nil {
				return mutationOutcome{}, nil, err
			}
			view := DeadlineView{Deadline: item, Current: true, Passed: !service.now().Before(deadlineAt), AttentionOnly: true}
			return mutationOutcome{Status: http.StatusCreated, Body: view, Version: project.Version, EventIDs: []string{eventID}}, nil, nil
		})
	return result, nil
}

func planningProjectionKey(kind, id string) string { return kind + ":" + id }

func forecastScopeKey(projectID string, deliverableID *string) string {
	scope := "project"
	if deliverableID != nil {
		scope = *deliverableID
	}
	return planningProjectionKey("forecast-head", projectID+":"+scope)
}

func sortPlanningSnapshot(snapshot *projectionSnapshot) {
	sort.Slice(snapshot.Deliverables, func(i, j int) bool { return snapshot.Deliverables[i].ID < snapshot.Deliverables[j].ID })
	sort.Slice(snapshot.Forecasts, func(i, j int) bool { return snapshot.Forecasts[i].ID < snapshot.Forecasts[j].ID })
	sort.Slice(snapshot.ForecastHeads, func(i, j int) bool {
		left, right := "project", "project"
		if snapshot.ForecastHeads[i].DeliverableID != nil {
			left = *snapshot.ForecastHeads[i].DeliverableID
		}
		if snapshot.ForecastHeads[j].DeliverableID != nil {
			right = *snapshot.ForecastHeads[j].DeliverableID
		}
		return snapshot.ForecastHeads[i].ProjectID+left < snapshot.ForecastHeads[j].ProjectID+right
	})
	sort.Slice(snapshot.Targets, func(i, j int) bool { return snapshot.Targets[i].ID < snapshot.Targets[j].ID })
	sort.Slice(snapshot.TargetHeads, func(i, j int) bool { return snapshot.TargetHeads[i].ProjectID < snapshot.TargetHeads[j].ProjectID })
	sort.Slice(snapshot.Deadlines, func(i, j int) bool { return snapshot.Deadlines[i].ID < snapshot.Deadlines[j].ID })
	sort.Slice(snapshot.DeadlineHeads, func(i, j int) bool { return snapshot.DeadlineHeads[i].ProjectID < snapshot.DeadlineHeads[j].ProjectID })
}
