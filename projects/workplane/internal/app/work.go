package app

import (
	"bytes"
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

type WorkItemRecord struct {
	ID             string  `json:"id"`
	OrganizationID string  `json:"organization_id"`
	ProjectID      string  `json:"project_id"`
	DeliverableID  *string `json:"deliverable_id"`
	Title          string  `json:"title"`
	Description    string  `json:"description"`
	State          string  `json:"state"`
	Priority       string  `json:"priority"`
	AssigneeID     *string `json:"assignee_id"`
	Version        int64   `json:"version"`
	CreatedBy      string  `json:"created_by"`
	CreatedAt      string  `json:"created_at"`
	UpdatedAt      string  `json:"updated_at"`
}

type WorkItem struct {
	WorkItemRecord
	Blocked         bool     `json:"blocked"`
	BlockingReasons []string `json:"blocking_reasons"`
}

type WorkItemDependency struct {
	ID               string `json:"id"`
	OrganizationID   string `json:"organization_id"`
	SourceWorkItemID string `json:"source_work_item_id"`
	TargetWorkItemID string `json:"target_work_item_id"`
	Kind             string `json:"kind"`
	Version          int64  `json:"version"`
	CreatedBy        string `json:"created_by"`
	CreatedAt        string `json:"created_at"`
}

type WorkItemEvent struct {
	WorkItem    WorkItemRecord `json:"work_item"`
	Command     string         `json:"command"`
	Reason      string         `json:"reason"`
	EvidenceIDs []string       `json:"evidence_ids"`
	FindingID   *string        `json:"finding_id"`
	Batch       bool           `json:"batch"`
}

type WorkDependencyEvent struct {
	Dependency WorkItemDependency `json:"dependency"`
	Source     WorkItemRecord     `json:"source"`
	Removed    bool               `json:"removed"`
}

type DependencyMutationResult struct {
	Dependency WorkItemDependency `json:"dependency"`
	Source     WorkItem           `json:"source"`
	Removed    bool               `json:"removed"`
}

type WorkItemBatchTransitionResult struct {
	Items []WorkItem `json:"items"`
}

type DependencyGraph struct {
	ProjectID    string               `json:"project_id"`
	WorkItems    []WorkItem           `json:"work_items"`
	Dependencies []WorkItemDependency `json:"dependencies"`
}

const selectWorkItem = `SELECT id,organization_id,project_id,deliverable_id,title,description,state::text,
	priority::text,assignee_id,version,created_by,created_at,updated_at FROM work_items`

const selectWorkDependency = `SELECT id,organization_id,source_work_item_id,target_work_item_id,kind::text,
	version,created_by,created_at FROM work_item_dependencies`

func scanWorkItem(row interface{ Scan(...any) error }) (WorkItemRecord, error) {
	var item WorkItemRecord
	var deliverableID, assigneeID sql.NullString
	var createdAt, updatedAt time.Time
	err := row.Scan(&item.ID, &item.OrganizationID, &item.ProjectID, &deliverableID, &item.Title,
		&item.Description, &item.State, &item.Priority, &assigneeID, &item.Version, &item.CreatedBy,
		&createdAt, &updatedAt)
	if deliverableID.Valid {
		item.DeliverableID = &deliverableID.String
	}
	if assigneeID.Valid {
		item.AssigneeID = &assigneeID.String
	}
	item.CreatedAt, item.UpdatedAt = createdAt.UTC().Format(timeFormat), updatedAt.UTC().Format(timeFormat)
	return item, err
}

func scanWorkDependency(row interface{ Scan(...any) error }) (WorkItemDependency, error) {
	var item WorkItemDependency
	var createdAt time.Time
	err := row.Scan(&item.ID, &item.OrganizationID, &item.SourceWorkItemID, &item.TargetWorkItemID,
		&item.Kind, &item.Version, &item.CreatedBy, &createdAt)
	item.CreatedAt = createdAt.UTC().Format(timeFormat)
	return item, err
}

func (service *Service) workItemProjectID(ctx context.Context, id string) (string, bool) {
	if !uuidPattern.MatchString(id) {
		return "", false
	}
	var projectID string
	if err := service.db.QueryRowContext(ctx, `SELECT project_id FROM work_items WHERE id=$1`, id).Scan(&projectID); err != nil {
		return "", false
	}
	return projectID, true
}

func workBlockingReasons(ctx context.Context, queryer databaseQueryer, item WorkItemRecord, finalStates map[string]string) ([]string, error) {
	reasons := make([]string, 0, 2)
	rows, err := queryer.QueryContext(ctx, `SELECT target.id,target.state::text
		FROM work_item_dependencies dependency
		JOIN work_items target ON target.id=dependency.target_work_item_id
		WHERE dependency.organization_id=$1 AND dependency.source_work_item_id=$2 AND dependency.kind='blocks'
		ORDER BY target.id`, item.OrganizationID, item.ID)
	if err != nil {
		return nil, err
	}
	dependencyBlocked := false
	for rows.Next() {
		var id, state string
		if err := rows.Scan(&id, &state); err != nil {
			_ = rows.Close()
			return nil, err
		}
		if replacement, ok := finalStates[id]; ok {
			state = replacement
		}
		if state != "done" && state != "cancelled" {
			dependencyBlocked = true
		}
	}
	if err := rows.Err(); err != nil {
		_ = rows.Close()
		return nil, err
	}
	if err := rows.Close(); err != nil {
		return nil, err
	}
	if dependencyBlocked {
		reasons = append(reasons, "dependency")
	}
	if item.DeliverableID != nil {
		var blocked bool
		err := queryer.QueryRowContext(ctx, `SELECT EXISTS (
			SELECT 1 FROM review_gates WHERE deliverable_id=$1 AND hard AND state<>'passed'
		)`, *item.DeliverableID).Scan(&blocked)
		if err != nil {
			return nil, err
		}
		if blocked {
			reasons = append(reasons, "hard_gate")
		}
	}
	return reasons, nil
}

func workItemView(ctx context.Context, queryer databaseQueryer, item WorkItemRecord, finalStates map[string]string) (WorkItem, error) {
	reasons, err := workBlockingReasons(ctx, queryer, item, finalStates)
	if err != nil {
		return WorkItem{}, err
	}
	if reasons == nil {
		reasons = []string{}
	}
	return WorkItem{WorkItemRecord: item, Blocked: len(reasons) > 0, BlockingReasons: reasons}, nil
}

// authorizeWorkProjectWith applies delegated authority as an intersection. The
// ordinary project check resolves the agent's current direct role (falling back
// to the delegated identity only when no direct role exists); the second check
// always resolves the human principal independently. A direct agent role can
// therefore narrow the delegation, but can never replace or widen the human's
// current project authority.
func (service *Service) authorizeWorkProjectWith(ctx context.Context, query projectAuthorizationQuery, actor Actor,
	projectID, organizationID, action, rid string) (generated.Response, bool) {
	if denied, ok := service.authorizeProjectWith(ctx, query, actor, projectID, organizationID, action, rid); !ok {
		return denied, false
	}
	if actor.Kind != "agent" {
		return generated.Response{}, true
	}
	if actor.PrincipalID == nil || *actor.PrincipalID == actor.ID {
		return problem(http.StatusForbidden, "forbidden", "Action denied", "The delegated principal is not available.", rid), false
	}
	delegated := Actor{ID: *actor.PrincipalID, Kind: "human", OrganizationID: actor.OrganizationID, Role: actor.DelegatedRole}
	return service.authorizeProjectWith(ctx, query, delegated, projectID, organizationID, action, rid)
}

func (service *Service) authorizeWorkProject(ctx context.Context, actor Actor, projectID, organizationID, action, rid string) (generated.Response, bool) {
	return service.authorizeWorkProjectWith(ctx, service.db, actor, projectID, organizationID, action, rid)
}

func (service *Service) GetWorkItem(ctx context.Context, request generated.Request) (generated.Response, error) {
	id := request.HTTPRequest.PathValue("id")
	projectID, found := service.workItemProjectID(ctx, id)
	actor, authResponse, ok := service.authenticate(ctx, request, "work.read", projectID)
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
	item, err := scanWorkItem(service.db.QueryRowContext(ctx, selectWorkItem+` WHERE id=$1 AND organization_id=$2`, id, actor.OrganizationID))
	if err != nil {
		return problem(http.StatusNotFound, "not_found", "Resource not found", "The requested resource is not available.", rid), nil
	}
	if denied, ok := service.authorizeWorkProject(ctx, actor, item.ProjectID, item.OrganizationID, "work.read", rid); !ok {
		return denied, nil
	}
	view, err := workItemView(ctx, service.db, item, nil)
	if err != nil {
		return serviceUnavailable(rid), nil
	}
	return generated.Response{Status: http.StatusOK, Headers: generated.ResponseHeaders{
		ETag: generated.VersionETag(fmt.Sprintf(`"%d"`, item.Version)), XRequestID: rid}, Body: view}, nil
}

func (service *Service) ListWorkItems(ctx context.Context, request generated.Request) (generated.Response, error) {
	projectID := request.HTTPRequest.PathValue("project_id")
	actor, authResponse, ok := service.authenticate(ctx, request, "work.read", projectID)
	if !ok {
		return authResponse, nil
	}
	rid := authResponse.Headers.XRequestID
	if len(strings.TrimSpace(string(request.Body))) > 0 || !uuidPattern.MatchString(projectID) {
		return problem(http.StatusNotFound, "not_found", "Resource not found", "The requested resource is not available.", rid), nil
	}
	if denied, ok := service.authorizeWorkProject(ctx, actor, projectID, actor.OrganizationID, "work.read", rid); !ok {
		return denied, nil
	}
	rows, err := service.db.QueryContext(ctx, selectWorkItem+` WHERE project_id=$1 AND organization_id=$2 ORDER BY created_at,id`, projectID, actor.OrganizationID)
	if err != nil {
		return serviceUnavailable(rid), nil
	}
	defer rows.Close()
	records := make([]WorkItemRecord, 0)
	for rows.Next() {
		item, err := scanWorkItem(rows)
		if err != nil {
			return serviceUnavailable(rid), nil
		}
		records = append(records, item)
	}
	if err := rows.Err(); err != nil {
		return serviceUnavailable(rid), nil
	}
	items := make([]WorkItem, 0, len(records))
	for _, item := range records {
		view, err := workItemView(ctx, service.db, item, nil)
		if err != nil {
			return serviceUnavailable(rid), nil
		}
		items = append(items, view)
	}
	return generated.Response{Status: http.StatusOK, Headers: generated.ResponseHeaders{XRequestID: rid}, Body: items}, nil
}

func validWorkPriority(priority string) bool {
	return priority == "low" || priority == "normal" || priority == "high" || priority == "critical"
}

func normalizeWorkItemCreate(input generated.WorkItemCreateInput) (generated.WorkItemCreateInput, bool) {
	input.Title, input.Description, input.Priority = strings.TrimSpace(input.Title), strings.TrimSpace(input.Description), strings.TrimSpace(input.Priority)
	if !validText(input.Title, 1, 200) || !validText(input.Description, 1, 4000) || !validWorkPriority(input.Priority) {
		return generated.WorkItemCreateInput{}, false
	}
	if input.DeliverableID != nil {
		value := strings.ToLower(strings.TrimSpace(*input.DeliverableID))
		if !uuidPattern.MatchString(value) {
			return generated.WorkItemCreateInput{}, false
		}
		input.DeliverableID = &value
	}
	return input, true
}

func parseExpectedVersion(value generated.VersionETag) (int64, bool) {
	match := versionETagPattern.FindStringSubmatch(string(value))
	if len(match) != 2 {
		return 0, false
	}
	parsed, err := strconv.ParseInt(match[1], 10, 64)
	return parsed, err == nil
}

func validIdempotencyKey(key string) bool {
	count := utf8.RuneCountInString(key)
	return count >= generated.IdempotencyKeyMinLength && count <= generated.IdempotencyKeyMaxLength
}

func (service *Service) prepareProjectWorkMutation(ctx context.Context, request generated.Request, action, projectID string, versioned bool) (Actor, string, int64, generated.Response, bool) {
	actor, authResponse, ok := service.authenticate(ctx, request, action, projectID)
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
	if !validIdempotencyKey(request.IdempotencyKey) {
		return Actor{}, "", 0, problem(http.StatusBadRequest, "invalid_request", "Idempotency key required", "Idempotency-Key must contain 16 to 128 characters.", rid), false
	}
	expected := int64(0)
	if versioned {
		var valid bool
		expected, valid = parseExpectedVersion(request.ExpectedVersion)
		if !valid {
			return Actor{}, "", 0, problem(http.StatusPreconditionRequired, "version_conflict", "Expected version required", "If-Match must contain the quoted aggregate version.", rid), false
		}
	}
	if denied, ok := service.authorizeWorkProject(ctx, actor, projectID, actor.OrganizationID, action, rid); !ok {
		return Actor{}, "", 0, denied, false
	}
	return actor, rid, expected, generated.Response{}, true
}

func (service *Service) prepareWorkItemMutation(ctx context.Context, request generated.Request, action string) (Actor, string, string, int64, generated.Response, bool) {
	id := request.HTTPRequest.PathValue("id")
	projectID, found := service.workItemProjectID(ctx, id)
	actor, rid, expected, denied, ok := service.prepareProjectWorkMutation(ctx, request, action, projectID, true)
	if !ok {
		return Actor{}, "", "", 0, denied, false
	}
	if !found || !uuidPattern.MatchString(id) {
		return Actor{}, "", "", 0, problem(http.StatusNotFound, "not_found", "Resource not found", "The requested resource is not available.", rid), false
	}
	return actor, rid, projectID, expected, generated.Response{}, true
}

func insertWorkEvent(ctx context.Context, tx *sql.Tx, service *Service, request generated.Request, actor Actor,
	rid string, item WorkItemRecord, eventType string, payload any, now time.Time, boundary string) (string, error) {
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
	encoded := canonicalJSON(payload)
	_, err = tx.ExecContext(ctx, `INSERT INTO domain_events
		(event_id,organization_id,aggregate_type,aggregate_id,aggregate_version,event_type,actor_kind,actor_id,principal_id,command_id,request_id,occurred_at,payload)
		VALUES ($1,$2,'work_item',$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)`, eventID, item.OrganizationID,
		item.ID, item.Version, eventType, actor.Kind, actor.ID, actor.PrincipalID, commandID, rid, now, encoded)
	if err != nil {
		return "", err
	}
	if service.config.FaultInjection {
		fault := request.HTTPRequest.Header.Get("X-Workplane-Fault")
		if fault == boundary || fault == "after-work-event" {
			return "", ErrInjectedCrash
		}
	}
	return eventID, nil
}

func workMutationFailure(err error, rid string) generated.Response {
	var databaseError *pq.Error
	if errors.As(err, &databaseError) {
		if databaseError.Code == "40001" || databaseError.Code == "40P01" {
			return problem(http.StatusConflict, "version_conflict", "Concurrent work-item update", "Another command changed a work item; retry from current versions.", rid)
		}
		if databaseError.Code == "P0001" && strings.Contains(databaseError.Message, "dependency_cycle") {
			return problem(http.StatusConflict, "dependency_cycle", "Blocking dependency cycle", "The blocking edge would create a direct or transitive cycle.", rid)
		}
	}
	return serviceUnavailable(rid)
}

func (service *Service) CreateWorkItem(ctx context.Context, request generated.Request) (generated.Response, error) {
	projectID := request.HTTPRequest.PathValue("project_id")
	actor, rid, _, denied, ok := service.prepareProjectWorkMutation(ctx, request, "work.edit", projectID, false)
	if !ok {
		return denied, nil
	}
	input, err := decodeStrict[generated.WorkItemCreateInput](request.Body)
	if err != nil {
		return problem(http.StatusBadRequest, "invalid_request", "Invalid work item", "The request body does not match the work-item contract.", rid), nil
	}
	input, valid := normalizeWorkItemCreate(input)
	if !valid {
		return problem(http.StatusBadRequest, "invalid_request", "Invalid work item", "Title, description, priority, or deliverable is invalid.", rid), nil
	}
	canonical := canonicalJSON(input)
	hash := requestHash([]byte(request.HTTPRequest.URL.EscapedPath()), canonical)
	tx, err := service.db.BeginTx(ctx, &sql.TxOptions{Isolation: sql.LevelSerializable})
	if err != nil {
		return serviceUnavailable(rid), nil
	}
	defer tx.Rollback()
	if _, err := tx.ExecContext(ctx, `SELECT pg_advisory_xact_lock(hashtextextended($1,0))`, actor.OrganizationID+actor.ID+"createWorkItem"+request.IdempotencyKey); err != nil {
		return serviceUnavailable(rid), nil
	}
	if denied, ok := service.authorizeWorkProjectWith(ctx, tx, actor, projectID, actor.OrganizationID, "work.edit", rid); !ok {
		return denied, nil
	}
	if replay, found, conflict := replayIdempotency(ctx, tx, actor.OrganizationID, actor.ID, "createWorkItem", request.IdempotencyKey, hash); found {
		if conflict {
			return problem(http.StatusConflict, "idempotency_conflict", "Idempotency key conflict", "The key was already used with a different target or request.", rid), nil
		}
		return replay, nil
	}
	var state string
	if err := tx.QueryRowContext(ctx, `SELECT state::text FROM projects WHERE id=$1 AND organization_id=$2 FOR SHARE`, projectID, actor.OrganizationID).Scan(&state); err != nil {
		return workMutationFailure(err, rid), nil
	}
	if state == "completed" || state == "stopped" || state == "cancelled" {
		return problem(http.StatusConflict, "invariant_violation", "Project is terminal", "Terminal projects do not accept new work items.", rid), nil
	}
	if input.DeliverableID != nil {
		var exists bool
		if err := tx.QueryRowContext(ctx, `SELECT EXISTS (SELECT 1 FROM deliverables WHERE id=$1 AND project_id=$2)`, *input.DeliverableID, projectID).Scan(&exists); err != nil {
			return workMutationFailure(err, rid), nil
		}
		if !exists {
			return problem(http.StatusNotFound, "not_found", "Resource not found", "The requested deliverable is not available.", rid), nil
		}
	}
	id, err := newUUID()
	if err != nil {
		return serviceUnavailable(rid), nil
	}
	now := service.now().UTC().Truncate(time.Microsecond)
	item := WorkItemRecord{ID: id, OrganizationID: actor.OrganizationID, ProjectID: projectID,
		DeliverableID: input.DeliverableID, Title: input.Title, Description: input.Description, State: "open",
		Priority: input.Priority, Version: 1, CreatedBy: actor.ID, CreatedAt: now.Format(timeFormat), UpdatedAt: now.Format(timeFormat)}
	_, err = tx.ExecContext(ctx, `INSERT INTO work_items
		(id,organization_id,project_id,deliverable_id,title,description,state,priority,version,created_by,created_at,updated_at)
		VALUES ($1,$2,$3,$4,$5,$6,'open',$7,1,$8,$9,$9)`, item.ID, item.OrganizationID, item.ProjectID,
		item.DeliverableID, item.Title, item.Description, item.Priority, item.CreatedBy, now)
	if err != nil {
		return workMutationFailure(err, rid), nil
	}
	if service.config.FaultInjection && request.HTTPRequest.Header.Get("X-Workplane-Fault") == "after-work-row" {
		return serviceUnavailable(rid), nil
	}
	request.HTTPRequest.Header.Set("X-Workplane-Command-ID", deterministicUUID(rid+request.IdempotencyKey))
	eventID, err := insertWorkEvent(ctx, tx, service, request, actor, rid, item, "work_item.created",
		WorkItemEvent{WorkItem: item, Command: "create", Reason: "", EvidenceIDs: []string{}}, now, "after-work-created-event")
	if err != nil {
		return workMutationFailure(err, rid), nil
	}
	view, err := workItemView(ctx, tx, item, nil)
	if err != nil {
		return workMutationFailure(err, rid), nil
	}
	body := canonicalJSON(view)
	etag := `"1"`
	if err := storeIdempotency(ctx, tx, actor.OrganizationID, actor.ID, "createWorkItem", request.IdempotencyKey,
		hash, http.StatusCreated, body, etag, rid, []string{eventID}, now); err != nil {
		return workMutationFailure(err, rid), nil
	}
	if err := tx.Commit(); err != nil {
		return workMutationFailure(err, rid), nil
	}
	return generated.Response{Status: http.StatusCreated, Headers: generated.ResponseHeaders{ETag: generated.VersionETag(etag), XRequestID: rid}, Body: view}, nil
}

type workItemMutationOutcome struct {
	Status   int
	Body     any
	Version  int64
	EventIDs []string
}

type workItemMutation func(*sql.Tx, *WorkItemRecord, Actor, string, time.Time) (workItemMutationOutcome, *generated.Response, error)

func (service *Service) executeWorkItemMutation(ctx context.Context, request generated.Request, actor Actor, rid,
	projectID, operation string, expected int64, canonical []byte, mutate workItemMutation) generated.Response {
	hash := requestHash([]byte(request.HTTPRequest.URL.EscapedPath()), canonical, []byte(request.ExpectedVersion))
	tx, err := service.db.BeginTx(ctx, &sql.TxOptions{Isolation: sql.LevelSerializable})
	if err != nil {
		return serviceUnavailable(rid)
	}
	defer tx.Rollback()
	if _, err := tx.ExecContext(ctx, `SELECT pg_advisory_xact_lock(hashtextextended($1,0))`, actor.OrganizationID+actor.ID+operation+request.IdempotencyKey); err != nil {
		return serviceUnavailable(rid)
	}
	id := request.HTTPRequest.PathValue("id")
	item, err := scanWorkItem(tx.QueryRowContext(ctx, selectWorkItem+` WHERE id=$1 AND organization_id=$2 FOR UPDATE`, id, actor.OrganizationID))
	if err != nil {
		if errors.Is(err, sql.ErrNoRows) {
			return problem(http.StatusNotFound, "not_found", "Resource not found", "The requested resource is not available.", rid)
		}
		return workMutationFailure(err, rid)
	}
	if item.ProjectID != projectID {
		return problem(http.StatusNotFound, "not_found", "Resource not found", "The requested resource is not available.", rid)
	}
	action := map[string]string{"updateWorkItem": "work.edit", "assignWorkItem": "work.assign", "transitionWorkItem": "work.transition"}[operation]
	if denied, ok := service.authorizeWorkProjectWith(ctx, tx, actor, item.ProjectID, item.OrganizationID, action, rid); !ok {
		return denied
	}
	if replay, found, conflict := replayIdempotency(ctx, tx, actor.OrganizationID, actor.ID, operation, request.IdempotencyKey, hash); found {
		if conflict {
			return problem(http.StatusConflict, "idempotency_conflict", "Idempotency key conflict", "The key was already used with a different target or request.", rid)
		}
		return replay
	}
	if item.Version != expected {
		return problem(http.StatusConflict, "version_conflict", "Work item version changed", fmt.Sprintf("Expected version %d; current version is %d.", expected, item.Version), rid)
	}
	if item.State == "done" || item.State == "cancelled" {
		return problem(http.StatusConflict, "invariant_violation", "Work item is terminal", "Terminal work items are immutable.", rid)
	}
	now := service.now().UTC().Truncate(time.Microsecond)
	commandID, err := newUUID()
	if err != nil {
		return serviceUnavailable(rid)
	}
	request.HTTPRequest.Header.Set("X-Workplane-Command-ID", commandID)
	outcome, rejected, err := mutate(tx, &item, actor, rid, now)
	if rejected != nil {
		return *rejected
	}
	if err != nil {
		return workMutationFailure(err, rid)
	}
	etag := fmt.Sprintf(`"%d"`, outcome.Version)
	body := canonicalJSON(outcome.Body)
	if err := storeIdempotency(ctx, tx, actor.OrganizationID, actor.ID, operation, request.IdempotencyKey, hash,
		outcome.Status, body, etag, rid, outcome.EventIDs, now); err != nil {
		return workMutationFailure(err, rid)
	}
	if err := tx.Commit(); err != nil {
		return workMutationFailure(err, rid)
	}
	return generated.Response{Status: outcome.Status, Headers: generated.ResponseHeaders{ETag: generated.VersionETag(etag), XRequestID: rid}, Body: outcome.Body}
}

func normalizeWorkItemUpdate(body json.RawMessage) (generated.WorkItemUpdateInput, bool) {
	input, err := decodeStrict[generated.WorkItemUpdateInput](body)
	if err != nil {
		return generated.WorkItemUpdateInput{}, false
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(body, &raw); err != nil || len(raw) == 0 {
		return generated.WorkItemUpdateInput{}, false
	}
	for _, value := range raw {
		if bytes.Equal(bytes.TrimSpace(value), []byte("null")) {
			return generated.WorkItemUpdateInput{}, false
		}
	}
	if input.Title != nil {
		value := strings.TrimSpace(*input.Title)
		if !validText(value, 1, 200) {
			return generated.WorkItemUpdateInput{}, false
		}
		input.Title = &value
	}
	if input.Description != nil {
		value := strings.TrimSpace(*input.Description)
		if !validText(value, 1, 4000) {
			return generated.WorkItemUpdateInput{}, false
		}
		input.Description = &value
	}
	if input.Priority != nil {
		value := strings.TrimSpace(*input.Priority)
		if !validWorkPriority(value) {
			return generated.WorkItemUpdateInput{}, false
		}
		input.Priority = &value
	}
	if input.DeliverableID != nil {
		value := strings.ToLower(strings.TrimSpace(*input.DeliverableID))
		if !uuidPattern.MatchString(value) {
			return generated.WorkItemUpdateInput{}, false
		}
		input.DeliverableID = &value
	}
	return input, true
}

func (service *Service) UpdateWorkItem(ctx context.Context, request generated.Request) (generated.Response, error) {
	actor, rid, projectID, expected, denied, ok := service.prepareWorkItemMutation(ctx, request, "work.edit")
	if !ok {
		return denied, nil
	}
	input, valid := normalizeWorkItemUpdate(request.Body)
	if !valid {
		return problem(http.StatusBadRequest, "invalid_request", "Invalid work item update", "Provide at least one declared non-null field with a valid value.", rid), nil
	}
	result := service.executeWorkItemMutation(ctx, request, actor, rid, projectID, "updateWorkItem", expected, canonicalJSON(input),
		func(tx *sql.Tx, item *WorkItemRecord, actor Actor, rid string, now time.Time) (workItemMutationOutcome, *generated.Response, error) {
			if input.DeliverableID != nil {
				var exists bool
				if err := tx.QueryRowContext(ctx, `SELECT EXISTS (SELECT 1 FROM deliverables WHERE id=$1 AND project_id=$2)`, *input.DeliverableID, item.ProjectID).Scan(&exists); err != nil {
					return workItemMutationOutcome{}, nil, err
				}
				if !exists {
					response := problem(http.StatusNotFound, "not_found", "Resource not found", "The requested deliverable is not available.", rid)
					return workItemMutationOutcome{}, &response, nil
				}
				item.DeliverableID = input.DeliverableID
			}
			if input.Title != nil {
				item.Title = *input.Title
			}
			if input.Description != nil {
				item.Description = *input.Description
			}
			if input.Priority != nil {
				item.Priority = *input.Priority
			}
			item.Version++
			item.UpdatedAt = now.Format(timeFormat)
			_, err := tx.ExecContext(ctx, `UPDATE work_items SET deliverable_id=$1,title=$2,description=$3,priority=$4,version=$5,updated_at=$6 WHERE id=$7`,
				item.DeliverableID, item.Title, item.Description, item.Priority, item.Version, now, item.ID)
			if err != nil {
				return workItemMutationOutcome{}, nil, err
			}
			if service.config.FaultInjection && request.HTTPRequest.Header.Get("X-Workplane-Fault") == "after-work-row" {
				return workItemMutationOutcome{}, nil, ErrInjectedCrash
			}
			eventID, err := insertWorkEvent(ctx, tx, service, request, actor, rid, *item, "work_item.updated",
				WorkItemEvent{WorkItem: *item, Command: "update", EvidenceIDs: []string{}}, now, "after-work-updated-event")
			if err != nil {
				return workItemMutationOutcome{}, nil, err
			}
			view, err := workItemView(ctx, tx, *item, nil)
			return workItemMutationOutcome{Status: http.StatusOK, Body: view, Version: item.Version, EventIDs: []string{eventID}}, nil, err
		})
	return result, nil
}

func (service *Service) AssignWorkItem(ctx context.Context, request generated.Request) (generated.Response, error) {
	actor, rid, projectID, expected, denied, ok := service.prepareWorkItemMutation(ctx, request, "work.assign")
	if !ok {
		return denied, nil
	}
	input, err := decodeStrict[generated.WorkItemAssignInput](request.Body)
	input.AssigneeID = strings.ToLower(strings.TrimSpace(input.AssigneeID))
	if err != nil || !uuidPattern.MatchString(input.AssigneeID) {
		return problem(http.StatusBadRequest, "invalid_request", "Invalid assignee", "assignee_id must name an active organization principal.", rid), nil
	}
	result := service.executeWorkItemMutation(ctx, request, actor, rid, projectID, "assignWorkItem", expected, canonicalJSON(input),
		func(tx *sql.Tx, item *WorkItemRecord, actor Actor, rid string, now time.Time) (workItemMutationOutcome, *generated.Response, error) {
			var active bool
			err := tx.QueryRowContext(ctx, `SELECT EXISTS (
				SELECT 1 FROM principals principal JOIN organization_memberships membership ON membership.principal_id=principal.id
				WHERE principal.id=$1 AND principal.status='active' AND membership.organization_id=$2
			)`, input.AssigneeID, item.OrganizationID).Scan(&active)
			if err != nil {
				return workItemMutationOutcome{}, nil, err
			}
			if !active {
				response := problem(http.StatusNotFound, "not_found", "Resource not found", "The requested assignee is not available.", rid)
				return workItemMutationOutcome{}, &response, nil
			}
			item.AssigneeID = &input.AssigneeID
			item.Version++
			item.UpdatedAt = now.Format(timeFormat)
			if _, err := tx.ExecContext(ctx, `UPDATE work_items SET assignee_id=$1,version=$2,updated_at=$3 WHERE id=$4`, input.AssigneeID, item.Version, now, item.ID); err != nil {
				return workItemMutationOutcome{}, nil, err
			}
			if service.config.FaultInjection && request.HTTPRequest.Header.Get("X-Workplane-Fault") == "after-work-row" {
				return workItemMutationOutcome{}, nil, ErrInjectedCrash
			}
			eventID, err := insertWorkEvent(ctx, tx, service, request, actor, rid, *item, "work_item.assigned",
				WorkItemEvent{WorkItem: *item, Command: "assign", Reason: "", EvidenceIDs: []string{}}, now, "after-work-assigned-event")
			if err != nil {
				return workItemMutationOutcome{}, nil, err
			}
			view, err := workItemView(ctx, tx, *item, nil)
			return workItemMutationOutcome{Status: http.StatusOK, Body: view, Version: item.Version, EventIDs: []string{eventID}}, nil, err
		})
	return result, nil
}

func normalizeWorkTransition(command, reason string, evidenceIDs []string, findingID *string) (string, string, []string, *string, bool) {
	command, reason = strings.TrimSpace(command), strings.TrimSpace(reason)
	if !validText(reason, 1, 4000) || evidenceIDs == nil || len(evidenceIDs) > 64 {
		return "", "", nil, nil, false
	}
	var ok bool
	evidenceIDs, ok = normalizeUUIDList(evidenceIDs, 0, 64)
	if !ok {
		return "", "", nil, nil, false
	}
	if findingID != nil {
		value := strings.ToLower(strings.TrimSpace(*findingID))
		if !uuidPattern.MatchString(value) {
			return "", "", nil, nil, false
		}
		findingID = &value
	}
	if command != "start" && command != "request_review" && command != "bounce" && command != "accept" && command != "cancel" {
		return "", "", nil, nil, false
	}
	switch command {
	case "request_review", "accept":
		if len(evidenceIDs) == 0 || findingID != nil {
			return "", "", nil, nil, false
		}
	case "bounce":
		if len(evidenceIDs) != 0 || findingID == nil {
			return "", "", nil, nil, false
		}
	default:
		if len(evidenceIDs) != 0 || findingID != nil {
			return "", "", nil, nil, false
		}
	}
	return command, reason, evidenceIDs, findingID, true
}

func workTransitionTarget(state, command string) (string, string, bool) {
	switch {
	case state == "open" && command == "start":
		return "in_progress", "work_item.started", true
	case state == "in_progress" && command == "request_review":
		return "in_review", "work_item.review_requested", true
	case state == "in_review" && command == "bounce":
		return "in_progress", "work_item.bounced", true
	case state == "in_review" && command == "accept":
		return "done", "work_item.accepted", true
	case (state == "open" || state == "in_progress" || state == "in_review") && command == "cancel":
		return "cancelled", "work_item.cancelled", true
	default:
		return "", "", false
	}
}

func (service *Service) validateWorkTransition(ctx context.Context, tx *sql.Tx, item WorkItemRecord, command string,
	evidenceIDs []string, findingID *string, finalStates map[string]string, rid string) *generated.Response {
	if _, _, ok := workTransitionTarget(item.State, command); !ok {
		response := problem(http.StatusConflict, "invariant_violation", "Invalid work-item transition", "The fixed lifecycle does not declare this state and command edge.", rid)
		return &response
	}
	if command == "start" {
		if item.AssigneeID == nil {
			response := problem(http.StatusConflict, "invariant_violation", "Active assignee required", "Starting work requires an active assignee.", rid)
			return &response
		}
		var active bool
		if err := tx.QueryRowContext(ctx, `SELECT EXISTS (
			SELECT 1 FROM principals principal JOIN organization_memberships membership ON membership.principal_id=principal.id
			WHERE principal.id=$1 AND principal.status='active' AND membership.organization_id=$2
		)`, *item.AssigneeID, item.OrganizationID).Scan(&active); err != nil || !active {
			response := problem(http.StatusConflict, "invariant_violation", "Active assignee required", "Starting work requires an active assignee.", rid)
			return &response
		}
	}
	if command == "request_review" || command == "accept" {
		if len(evidenceIDs) == 0 {
			response := problem(http.StatusConflict, "invariant_violation", "Evidence required", "This transition requires current project evidence.", rid)
			return &response
		}
		items, err := loadCurrentEvidence(ctx, tx, item.ProjectID, evidenceIDs)
		if err != nil || len(items) != len(evidenceIDs) {
			response := problem(http.StatusNotFound, "not_found", "Resource not found", "The requested evidence is not available.", rid)
			return &response
		}
	}
	if command == "bounce" {
		if findingID == nil || item.DeliverableID == nil {
			response := problem(http.StatusConflict, "invariant_violation", "Actionable finding required", "Bounce requires an open blocking finding on the linked deliverable.", rid)
			return &response
		}
		var exists bool
		if err := tx.QueryRowContext(ctx, `SELECT EXISTS (
			SELECT 1 FROM review_findings WHERE id=$1 AND project_id=$2 AND deliverable_id=$3 AND state='open' AND blocking
		)`, *findingID, item.ProjectID, *item.DeliverableID).Scan(&exists); err != nil || !exists {
			response := problem(http.StatusConflict, "invariant_violation", "Actionable finding required", "Bounce requires an open blocking finding on the linked deliverable.", rid)
			return &response
		}
	}
	if command == "start" || command == "request_review" || command == "accept" {
		reasons, err := workBlockingReasons(ctx, tx, item, finalStates)
		if err != nil {
			response := serviceUnavailable(rid)
			return &response
		}
		if len(reasons) > 0 {
			response := problem(http.StatusConflict, "gate_blocked", "Work item is blocked", "Resolve every blocking dependency and hard gate before advancing work.", rid)
			return &response
		}
	}
	return nil
}

func (service *Service) TransitionWorkItem(ctx context.Context, request generated.Request) (generated.Response, error) {
	actor, rid, projectID, expected, denied, ok := service.prepareWorkItemMutation(ctx, request, "work.transition")
	if !ok {
		return denied, nil
	}
	input, err := decodeStrict[generated.WorkItemTransitionInput](request.Body)
	if err != nil {
		return problem(http.StatusBadRequest, "invalid_request", "Invalid transition", "The request body does not match the transition contract.", rid), nil
	}
	input.Command, input.Reason, input.EvidenceIDs, input.FindingID, ok = normalizeWorkTransition(input.Command, input.Reason, input.EvidenceIDs, input.FindingID)
	if !ok {
		return problem(http.StatusBadRequest, "invalid_request", "Invalid transition", "command, reason, evidence_ids, or finding_id is invalid.", rid), nil
	}
	result := service.executeWorkItemMutation(ctx, request, actor, rid, projectID, "transitionWorkItem", expected, canonicalJSON(input),
		func(tx *sql.Tx, item *WorkItemRecord, actor Actor, rid string, now time.Time) (workItemMutationOutcome, *generated.Response, error) {
			if rejected := service.validateWorkTransition(ctx, tx, *item, input.Command, input.EvidenceIDs, input.FindingID, nil, rid); rejected != nil {
				return workItemMutationOutcome{}, rejected, nil
			}
			target, eventType, _ := workTransitionTarget(item.State, input.Command)
			item.State, item.Version, item.UpdatedAt = target, item.Version+1, now.Format(timeFormat)
			if _, err := tx.ExecContext(ctx, `UPDATE work_items SET state=$1,version=$2,updated_at=$3 WHERE id=$4`, item.State, item.Version, now, item.ID); err != nil {
				return workItemMutationOutcome{}, nil, err
			}
			if service.config.FaultInjection && request.HTTPRequest.Header.Get("X-Workplane-Fault") == "after-work-row" {
				return workItemMutationOutcome{}, nil, ErrInjectedCrash
			}
			eventID, err := insertWorkEvent(ctx, tx, service, request, actor, rid, *item, eventType,
				WorkItemEvent{WorkItem: *item, Command: input.Command, Reason: input.Reason, EvidenceIDs: input.EvidenceIDs, FindingID: input.FindingID}, now, "after-work-transition-event")
			if err != nil {
				return workItemMutationOutcome{}, nil, err
			}
			view, err := workItemView(ctx, tx, *item, nil)
			return workItemMutationOutcome{Status: http.StatusOK, Body: view, Version: item.Version, EventIDs: []string{eventID}}, nil, err
		})
	return result, nil
}

func (service *Service) authorizeDependencyProject(ctx context.Context, tx *sql.Tx, actor Actor, projectID, rid string) (generated.Response, bool) {
	if actor.Kind == "agent" && !agentProjectRestrictionAllows(actor.ProjectIDs, projectID) {
		return problem(http.StatusNotFound, "not_found", "Resource not found", "The requested resource is not available.", rid), false
	}
	return service.authorizeWorkProjectWith(ctx, tx, actor, projectID, actor.OrganizationID, "dependency.edit", rid)
}

func blockingPathExists(ctx context.Context, tx *sql.Tx, organizationID, fromID, toID string) (bool, error) {
	var exists bool
	err := tx.QueryRowContext(ctx, `WITH RECURSIVE reachable(id) AS (
		SELECT target_work_item_id FROM work_item_dependencies
		WHERE organization_id=$1 AND kind='blocks' AND source_work_item_id=$2
		UNION
		SELECT dependency.target_work_item_id FROM work_item_dependencies dependency
		JOIN reachable ON dependency.source_work_item_id=reachable.id
		WHERE dependency.organization_id=$1 AND dependency.kind='blocks'
	) SELECT EXISTS (SELECT 1 FROM reachable WHERE id=$3)`, organizationID, fromID, toID).Scan(&exists)
	return exists, err
}

func (service *Service) AddWorkItemDependency(ctx context.Context, request generated.Request) (generated.Response, error) {
	actor, rid, projectID, expected, denied, ok := service.prepareWorkItemMutation(ctx, request, "dependency.edit")
	if !ok {
		return denied, nil
	}
	input, err := decodeStrict[generated.DependencyAddInput](request.Body)
	input.TargetWorkItemID, input.Kind = strings.ToLower(strings.TrimSpace(input.TargetWorkItemID)), strings.TrimSpace(input.Kind)
	if err != nil || !uuidPattern.MatchString(input.TargetWorkItemID) || input.ExpectedTargetVersion < 1 ||
		(input.Kind != "blocks" && input.Kind != "relates" && input.Kind != "caused-by") {
		return problem(http.StatusBadRequest, "invalid_request", "Invalid dependency", "target, kind, and expected target version are required.", rid), nil
	}
	hash := requestHash([]byte(request.HTTPRequest.URL.EscapedPath()), canonicalJSON(input), []byte(request.ExpectedVersion))
	tx, err := service.db.BeginTx(ctx, &sql.TxOptions{Isolation: sql.LevelSerializable})
	if err != nil {
		return serviceUnavailable(rid), nil
	}
	defer tx.Rollback()
	if _, err := tx.ExecContext(ctx, `SELECT pg_advisory_xact_lock(hashtextextended($1,0))`, actor.OrganizationID+actor.ID+"addWorkItemDependency"+request.IdempotencyKey); err != nil {
		return serviceUnavailable(rid), nil
	}
	if _, err := tx.ExecContext(ctx, `SELECT pg_advisory_xact_lock(hashtextextended($1,0))`, actor.OrganizationID+":work-dependency-graph"); err != nil {
		return serviceUnavailable(rid), nil
	}
	source, err := scanWorkItem(tx.QueryRowContext(ctx, selectWorkItem+` WHERE id=$1 AND organization_id=$2 FOR UPDATE`, request.HTTPRequest.PathValue("id"), actor.OrganizationID))
	if err != nil {
		if !errors.Is(err, sql.ErrNoRows) {
			return workMutationFailure(err, rid), nil
		}
		return problem(http.StatusNotFound, "not_found", "Resource not found", "The requested resource is not available.", rid), nil
	}
	if source.ProjectID != projectID {
		return problem(http.StatusNotFound, "not_found", "Resource not found", "The requested resource is not available.", rid), nil
	}
	if denied, ok := service.authorizeWorkProjectWith(ctx, tx, actor, source.ProjectID, source.OrganizationID, "dependency.edit", rid); !ok {
		return denied, nil
	}
	target, err := scanWorkItem(tx.QueryRowContext(ctx, selectWorkItem+` WHERE id=$1 AND organization_id=$2 FOR UPDATE`, input.TargetWorkItemID, actor.OrganizationID))
	if err != nil {
		if !errors.Is(err, sql.ErrNoRows) {
			return workMutationFailure(err, rid), nil
		}
		return problem(http.StatusNotFound, "not_found", "Resource not found", "The requested dependency target is not available.", rid), nil
	}
	if denied, ok := service.authorizeDependencyProject(ctx, tx, actor, target.ProjectID, rid); !ok {
		return denied, nil
	}
	if replay, found, conflict := replayIdempotency(ctx, tx, actor.OrganizationID, actor.ID, "addWorkItemDependency", request.IdempotencyKey, hash); found {
		if conflict {
			return problem(http.StatusConflict, "idempotency_conflict", "Idempotency key conflict", "The key was already used with a different target or request.", rid), nil
		}
		return replay, nil
	}
	if source.Version != expected || target.Version != input.ExpectedTargetVersion {
		return problem(http.StatusConflict, "version_conflict", "Work item version changed", "A dependency endpoint no longer has its expected version.", rid), nil
	}
	if source.State == "done" || source.State == "cancelled" {
		return problem(http.StatusConflict, "invariant_violation", "Work item is terminal", "Terminal work items cannot add dependencies.", rid), nil
	}
	if source.ID == target.ID {
		return problem(http.StatusConflict, "dependency_cycle", "Blocking dependency cycle", "A work item cannot depend on itself.", rid), nil
	}
	var duplicate bool
	if err := tx.QueryRowContext(ctx, `SELECT EXISTS (SELECT 1 FROM work_item_dependencies WHERE source_work_item_id=$1 AND target_work_item_id=$2)`, source.ID, target.ID).Scan(&duplicate); err != nil {
		return workMutationFailure(err, rid), nil
	}
	if duplicate {
		return problem(http.StatusConflict, "invariant_violation", "Duplicate dependency target", "An ordered pair can carry exactly one dependency kind.", rid), nil
	}
	if input.Kind == "blocks" {
		cycle, err := blockingPathExists(ctx, tx, actor.OrganizationID, target.ID, source.ID)
		if err != nil {
			return workMutationFailure(err, rid), nil
		}
		if cycle {
			return problem(http.StatusConflict, "dependency_cycle", "Blocking dependency cycle", "The blocking edge would create a direct or transitive cycle.", rid), nil
		}
	}
	dependencyID, err := newUUID()
	if err != nil {
		return serviceUnavailable(rid), nil
	}
	now := service.now().UTC().Truncate(time.Microsecond)
	dependency := WorkItemDependency{ID: dependencyID, OrganizationID: actor.OrganizationID, SourceWorkItemID: source.ID,
		TargetWorkItemID: target.ID, Kind: input.Kind, Version: 1, CreatedBy: actor.ID, CreatedAt: now.Format(timeFormat)}
	if _, err := tx.ExecContext(ctx, `INSERT INTO work_item_dependencies
		(id,organization_id,source_work_item_id,target_work_item_id,kind,version,created_by,created_at)
		VALUES ($1,$2,$3,$4,$5,1,$6,$7)`, dependency.ID, dependency.OrganizationID, dependency.SourceWorkItemID,
		dependency.TargetWorkItemID, dependency.Kind, dependency.CreatedBy, now); err != nil {
		return workMutationFailure(err, rid), nil
	}
	source.Version++
	source.UpdatedAt = now.Format(timeFormat)
	if _, err := tx.ExecContext(ctx, `UPDATE work_items SET version=$1,updated_at=$2 WHERE id=$3`, source.Version, now, source.ID); err != nil {
		return workMutationFailure(err, rid), nil
	}
	if service.config.FaultInjection && request.HTTPRequest.Header.Get("X-Workplane-Fault") == "after-dependency-row" {
		return serviceUnavailable(rid), nil
	}
	commandID, _ := newUUID()
	request.HTTPRequest.Header.Set("X-Workplane-Command-ID", commandID)
	eventID, err := insertWorkEvent(ctx, tx, service, request, actor, rid, source, "dependency.added",
		WorkDependencyEvent{Dependency: dependency, Source: source, Removed: false}, now, "after-dependency-event")
	if err != nil {
		return workMutationFailure(err, rid), nil
	}
	view, err := workItemView(ctx, tx, source, nil)
	if err != nil {
		return workMutationFailure(err, rid), nil
	}
	result := DependencyMutationResult{Dependency: dependency, Source: view, Removed: false}
	etag := fmt.Sprintf(`"%d"`, source.Version)
	if err := storeIdempotency(ctx, tx, actor.OrganizationID, actor.ID, "addWorkItemDependency", request.IdempotencyKey,
		hash, http.StatusCreated, canonicalJSON(result), etag, rid, []string{eventID}, now); err != nil {
		return workMutationFailure(err, rid), nil
	}
	if err := tx.Commit(); err != nil {
		return workMutationFailure(err, rid), nil
	}
	return generated.Response{Status: http.StatusCreated, Headers: generated.ResponseHeaders{ETag: generated.VersionETag(etag), XRequestID: rid}, Body: result}, nil
}

func (service *Service) RemoveWorkItemDependency(ctx context.Context, request generated.Request) (generated.Response, error) {
	actor, rid, projectID, expected, denied, ok := service.prepareWorkItemMutation(ctx, request, "dependency.edit")
	if !ok {
		return denied, nil
	}
	dependencyID := strings.ToLower(strings.TrimSpace(request.HTTPRequest.PathValue("dependency_id")))
	input, err := decodeStrict[generated.DependencyRemoveInput](request.Body)
	if err != nil || !uuidPattern.MatchString(dependencyID) || input.ExpectedDependencyVersion != 1 || input.ExpectedTargetVersion < 1 {
		return problem(http.StatusBadRequest, "invalid_request", "Invalid dependency removal", "Dependency and endpoint versions are required.", rid), nil
	}
	hash := requestHash([]byte(request.HTTPRequest.URL.EscapedPath()), canonicalJSON(input), []byte(request.ExpectedVersion))
	tx, err := service.db.BeginTx(ctx, &sql.TxOptions{Isolation: sql.LevelSerializable})
	if err != nil {
		return serviceUnavailable(rid), nil
	}
	defer tx.Rollback()
	if _, err := tx.ExecContext(ctx, `SELECT pg_advisory_xact_lock(hashtextextended($1,0))`, actor.OrganizationID+actor.ID+"removeWorkItemDependency"+request.IdempotencyKey); err != nil {
		return serviceUnavailable(rid), nil
	}
	if _, err := tx.ExecContext(ctx, `SELECT pg_advisory_xact_lock(hashtextextended($1,0))`, actor.OrganizationID+":work-dependency-graph"); err != nil {
		return serviceUnavailable(rid), nil
	}
	source, err := scanWorkItem(tx.QueryRowContext(ctx, selectWorkItem+` WHERE id=$1 AND organization_id=$2 FOR UPDATE`, request.HTTPRequest.PathValue("id"), actor.OrganizationID))
	if err != nil {
		if !errors.Is(err, sql.ErrNoRows) {
			return workMutationFailure(err, rid), nil
		}
		return problem(http.StatusNotFound, "not_found", "Resource not found", "The requested resource is not available.", rid), nil
	}
	if source.ProjectID != projectID {
		return problem(http.StatusNotFound, "not_found", "Resource not found", "The requested resource is not available.", rid), nil
	}
	if denied, ok := service.authorizeWorkProjectWith(ctx, tx, actor, source.ProjectID, source.OrganizationID, "dependency.edit", rid); !ok {
		return denied, nil
	}
	// Removal deletes the edge itself, so an exact retry cannot rediscover its
	// target from the live graph. Read the stored result first, then re-run the
	// target's current authorization before returning it. This preserves both
	// exact idempotency and the rule that stale success never bypasses a newly
	// revoked cross-project permission.
	if replay, found, conflict := replayIdempotency(ctx, tx, actor.OrganizationID, actor.ID, "removeWorkItemDependency", request.IdempotencyKey, hash); found {
		if conflict {
			return problem(http.StatusConflict, "idempotency_conflict", "Idempotency key conflict", "The key was already used with a different target or request.", rid), nil
		}
		var stored DependencyMutationResult
		encoded, ok := replay.Body.(json.RawMessage)
		if !ok || json.Unmarshal(encoded, &stored) != nil || !uuidPattern.MatchString(stored.Dependency.TargetWorkItemID) {
			return serviceUnavailable(rid), nil
		}
		target, err := scanWorkItem(tx.QueryRowContext(ctx, selectWorkItem+` WHERE id=$1 AND organization_id=$2 FOR UPDATE`, stored.Dependency.TargetWorkItemID, actor.OrganizationID))
		if err != nil {
			if !errors.Is(err, sql.ErrNoRows) {
				return workMutationFailure(err, rid), nil
			}
			return problem(http.StatusNotFound, "not_found", "Resource not found", "The requested dependency target is not available.", rid), nil
		}
		if denied, ok := service.authorizeDependencyProject(ctx, tx, actor, target.ProjectID, rid); !ok {
			return denied, nil
		}
		return replay, nil
	}
	dependency, err := scanWorkDependency(tx.QueryRowContext(ctx, selectWorkDependency+` WHERE id=$1 AND source_work_item_id=$2 AND organization_id=$3 FOR UPDATE`, dependencyID, source.ID, actor.OrganizationID))
	if err != nil {
		if !errors.Is(err, sql.ErrNoRows) {
			return workMutationFailure(err, rid), nil
		}
		return problem(http.StatusNotFound, "not_found", "Resource not found", "The requested dependency is not available.", rid), nil
	}
	target, err := scanWorkItem(tx.QueryRowContext(ctx, selectWorkItem+` WHERE id=$1 AND organization_id=$2 FOR UPDATE`, dependency.TargetWorkItemID, actor.OrganizationID))
	if err != nil {
		if !errors.Is(err, sql.ErrNoRows) {
			return workMutationFailure(err, rid), nil
		}
		return problem(http.StatusNotFound, "not_found", "Resource not found", "The requested dependency target is not available.", rid), nil
	}
	if denied, ok := service.authorizeDependencyProject(ctx, tx, actor, target.ProjectID, rid); !ok {
		return denied, nil
	}
	if source.Version != expected || target.Version != input.ExpectedTargetVersion || dependency.Version != input.ExpectedDependencyVersion {
		return problem(http.StatusConflict, "version_conflict", "Dependency version changed", "A dependency or endpoint no longer has its expected version.", rid), nil
	}
	if _, err := tx.ExecContext(ctx, `DELETE FROM work_item_dependencies WHERE id=$1`, dependency.ID); err != nil {
		return workMutationFailure(err, rid), nil
	}
	now := service.now().UTC().Truncate(time.Microsecond)
	source.Version++
	source.UpdatedAt = now.Format(timeFormat)
	if _, err := tx.ExecContext(ctx, `UPDATE work_items SET version=$1,updated_at=$2 WHERE id=$3`, source.Version, now, source.ID); err != nil {
		return workMutationFailure(err, rid), nil
	}
	if service.config.FaultInjection && request.HTTPRequest.Header.Get("X-Workplane-Fault") == "after-dependency-row" {
		return serviceUnavailable(rid), nil
	}
	commandID, _ := newUUID()
	request.HTTPRequest.Header.Set("X-Workplane-Command-ID", commandID)
	eventID, err := insertWorkEvent(ctx, tx, service, request, actor, rid, source, "dependency.removed",
		WorkDependencyEvent{Dependency: dependency, Source: source, Removed: true}, now, "after-dependency-event")
	if err != nil {
		return workMutationFailure(err, rid), nil
	}
	view, err := workItemView(ctx, tx, source, nil)
	if err != nil {
		return workMutationFailure(err, rid), nil
	}
	result := DependencyMutationResult{Dependency: dependency, Source: view, Removed: true}
	etag := fmt.Sprintf(`"%d"`, source.Version)
	if err := storeIdempotency(ctx, tx, actor.OrganizationID, actor.ID, "removeWorkItemDependency", request.IdempotencyKey,
		hash, http.StatusOK, canonicalJSON(result), etag, rid, []string{eventID}, now); err != nil {
		return workMutationFailure(err, rid), nil
	}
	if err := tx.Commit(); err != nil {
		return workMutationFailure(err, rid), nil
	}
	return generated.Response{Status: http.StatusOK, Headers: generated.ResponseHeaders{ETag: generated.VersionETag(etag), XRequestID: rid}, Body: result}, nil
}

func normalizeBatchTransitions(input generated.WorkItemBatchTransitionInput) (generated.WorkItemBatchTransitionInput, bool) {
	if input.Items == nil || len(input.Items) < 1 || len(input.Items) > 100 {
		return generated.WorkItemBatchTransitionInput{}, false
	}
	seen := make(map[string]bool, len(input.Items))
	for index := range input.Items {
		entry := &input.Items[index]
		entry.WorkItemID = strings.ToLower(strings.TrimSpace(entry.WorkItemID))
		if !uuidPattern.MatchString(entry.WorkItemID) || entry.ExpectedVersion < 1 || seen[entry.WorkItemID] {
			return generated.WorkItemBatchTransitionInput{}, false
		}
		seen[entry.WorkItemID] = true
		var ok bool
		entry.Command, entry.Reason, entry.EvidenceIDs, entry.FindingID, ok = normalizeWorkTransition(entry.Command, entry.Reason, entry.EvidenceIDs, entry.FindingID)
		if !ok {
			return generated.WorkItemBatchTransitionInput{}, false
		}
	}
	return input, true
}

func (service *Service) BatchTransitionWorkItems(ctx context.Context, request generated.Request) (generated.Response, error) {
	projectID := request.HTTPRequest.PathValue("project_id")
	actor, rid, _, denied, ok := service.prepareProjectWorkMutation(ctx, request, "work.transition", projectID, false)
	if !ok {
		return denied, nil
	}
	input, err := decodeStrict[generated.WorkItemBatchTransitionInput](request.Body)
	if err != nil {
		return problem(http.StatusBadRequest, "invalid_request", "Invalid batch transition", "The request body does not match the atomic batch contract.", rid), nil
	}
	input, ok = normalizeBatchTransitions(input)
	if !ok {
		return problem(http.StatusBadRequest, "invalid_request", "Invalid batch transition", "Items must be unique and carry complete current versions and transition members.", rid), nil
	}
	hash := requestHash([]byte(request.HTTPRequest.URL.EscapedPath()), canonicalJSON(input))
	tx, err := service.db.BeginTx(ctx, &sql.TxOptions{Isolation: sql.LevelSerializable})
	if err != nil {
		return serviceUnavailable(rid), nil
	}
	defer tx.Rollback()
	if _, err := tx.ExecContext(ctx, `SELECT pg_advisory_xact_lock(hashtextextended($1,0))`, actor.OrganizationID+actor.ID+"batchTransitionWorkItems"+request.IdempotencyKey); err != nil {
		return serviceUnavailable(rid), nil
	}
	if denied, ok := service.authorizeWorkProjectWith(ctx, tx, actor, projectID, actor.OrganizationID, "work.transition", rid); !ok {
		return denied, nil
	}
	if replay, found, conflict := replayIdempotency(ctx, tx, actor.OrganizationID, actor.ID, "batchTransitionWorkItems", request.IdempotencyKey, hash); found {
		if conflict {
			return problem(http.StatusConflict, "idempotency_conflict", "Idempotency key conflict", "The key was already used with a different target or request.", rid), nil
		}
		return replay, nil
	}
	ids := make([]string, len(input.Items))
	for index, entry := range input.Items {
		ids[index] = entry.WorkItemID
	}
	sort.Strings(ids)
	rows, err := tx.QueryContext(ctx, selectWorkItem+` WHERE id=ANY($1::uuid[]) AND organization_id=$2 ORDER BY id FOR UPDATE`, pq.Array(ids), actor.OrganizationID)
	if err != nil {
		return workMutationFailure(err, rid), nil
	}
	records := make(map[string]WorkItemRecord, len(ids))
	for rows.Next() {
		item, err := scanWorkItem(rows)
		if err != nil {
			_ = rows.Close()
			return serviceUnavailable(rid), nil
		}
		records[item.ID] = item
	}
	if err := rows.Err(); err != nil {
		_ = rows.Close()
		return serviceUnavailable(rid), nil
	}
	if err := rows.Close(); err != nil {
		return serviceUnavailable(rid), nil
	}
	if len(records) != len(input.Items) {
		return problem(http.StatusNotFound, "not_found", "Resource not found", "Every batch target must exist in the requested project.", rid), nil
	}
	finalStates := make(map[string]string, len(records))
	for _, entry := range input.Items {
		item := records[entry.WorkItemID]
		if item.ProjectID != projectID || item.OrganizationID != actor.OrganizationID {
			return problem(http.StatusNotFound, "not_found", "Resource not found", "Every batch target must exist in the requested project.", rid), nil
		}
		if item.Version != entry.ExpectedVersion {
			return problem(http.StatusConflict, "version_conflict", "Work item version changed", "At least one batch target no longer has its expected version.", rid), nil
		}
		target, _, valid := workTransitionTarget(item.State, entry.Command)
		if !valid {
			return problem(http.StatusConflict, "invariant_violation", "Invalid work-item transition", "At least one item does not admit its requested fixed lifecycle edge.", rid), nil
		}
		finalStates[item.ID] = target
	}
	for _, entry := range input.Items {
		item := records[entry.WorkItemID]
		if rejected := service.validateWorkTransition(ctx, tx, item, entry.Command, entry.EvidenceIDs, entry.FindingID, finalStates, rid); rejected != nil {
			return *rejected, nil
		}
	}
	now := service.now().UTC().Truncate(time.Microsecond)
	commandID, err := newUUID()
	if err != nil {
		return serviceUnavailable(rid), nil
	}
	request.HTTPRequest.Header.Set("X-Workplane-Command-ID", commandID)
	eventIDs := make([]string, 0, len(input.Items))
	result := WorkItemBatchTransitionResult{Items: make([]WorkItem, 0, len(input.Items))}
	for index, entry := range input.Items {
		item := records[entry.WorkItemID]
		target, eventType, _ := workTransitionTarget(item.State, entry.Command)
		item.State, item.Version, item.UpdatedAt = target, item.Version+1, now.Format(timeFormat)
		if _, err := tx.ExecContext(ctx, `UPDATE work_items SET state=$1,version=$2,updated_at=$3 WHERE id=$4`, item.State, item.Version, now, item.ID); err != nil {
			return workMutationFailure(err, rid), nil
		}
		if service.config.FaultInjection && request.HTTPRequest.Header.Get("X-Workplane-Fault") == "after-work-batch-row" && index == 0 {
			return serviceUnavailable(rid), nil
		}
		eventID, err := insertWorkEvent(ctx, tx, service, request, actor, rid, item, eventType,
			WorkItemEvent{WorkItem: item, Command: entry.Command, Reason: entry.Reason, EvidenceIDs: entry.EvidenceIDs, FindingID: entry.FindingID, Batch: true}, now, "after-work-batch-event")
		if err != nil {
			return workMutationFailure(err, rid), nil
		}
		eventIDs = append(eventIDs, eventID)
		records[item.ID] = item
	}
	for _, entry := range input.Items {
		view, err := workItemView(ctx, tx, records[entry.WorkItemID], nil)
		if err != nil {
			return workMutationFailure(err, rid), nil
		}
		result.Items = append(result.Items, view)
	}
	if err := storeIdempotency(ctx, tx, actor.OrganizationID, actor.ID, "batchTransitionWorkItems", request.IdempotencyKey,
		hash, http.StatusOK, canonicalJSON(result), "", rid, eventIDs, now); err != nil {
		return workMutationFailure(err, rid), nil
	}
	if err := tx.Commit(); err != nil {
		return workMutationFailure(err, rid), nil
	}
	return generated.Response{Status: http.StatusOK, Headers: generated.ResponseHeaders{XRequestID: rid}, Body: result}, nil
}

type graphDependencyRow struct {
	Dependency                   WorkItemDependency
	SourceProject, TargetProject string
}

func (service *Service) GetDependencyGraph(ctx context.Context, request generated.Request) (generated.Response, error) {
	projectID := request.HTTPRequest.PathValue("project_id")
	actor, authResponse, ok := service.authenticate(ctx, request, "dependency.read", projectID)
	if !ok {
		return authResponse, nil
	}
	rid := authResponse.Headers.XRequestID
	if len(strings.TrimSpace(string(request.Body))) > 0 || !uuidPattern.MatchString(projectID) {
		return problem(http.StatusNotFound, "not_found", "Resource not found", "The requested resource is not available.", rid), nil
	}
	if denied, ok := service.authorizeWorkProject(ctx, actor, projectID, actor.OrganizationID, "dependency.read", rid); !ok {
		return denied, nil
	}
	rows, err := service.db.QueryContext(ctx, `SELECT dependency.id,dependency.organization_id,dependency.source_work_item_id,
		dependency.target_work_item_id,dependency.kind::text,dependency.version,dependency.created_by,dependency.created_at,
		source.project_id,target.project_id
		FROM work_item_dependencies dependency
		JOIN work_items source ON source.id=dependency.source_work_item_id
		JOIN work_items target ON target.id=dependency.target_work_item_id
		WHERE dependency.organization_id=$1 AND (source.project_id=$2 OR target.project_id=$2)
		ORDER BY dependency.source_work_item_id,dependency.target_work_item_id,dependency.id`, actor.OrganizationID, projectID)
	if err != nil {
		return serviceUnavailable(rid), nil
	}
	dependencies := make([]graphDependencyRow, 0)
	for rows.Next() {
		var row graphDependencyRow
		var createdAt time.Time
		if err := rows.Scan(&row.Dependency.ID, &row.Dependency.OrganizationID, &row.Dependency.SourceWorkItemID,
			&row.Dependency.TargetWorkItemID, &row.Dependency.Kind, &row.Dependency.Version, &row.Dependency.CreatedBy,
			&createdAt, &row.SourceProject, &row.TargetProject); err != nil {
			_ = rows.Close()
			return serviceUnavailable(rid), nil
		}
		row.Dependency.CreatedAt = createdAt.UTC().Format(timeFormat)
		dependencies = append(dependencies, row)
	}
	if err := rows.Err(); err != nil {
		_ = rows.Close()
		return serviceUnavailable(rid), nil
	}
	if err := rows.Close(); err != nil {
		return serviceUnavailable(rid), nil
	}
	visibleProjects := map[string]bool{projectID: true}
	checkedProjects := map[string]bool{projectID: true}
	for _, edge := range dependencies {
		for _, candidate := range []string{edge.SourceProject, edge.TargetProject} {
			if checkedProjects[candidate] {
				continue
			}
			checkedProjects[candidate] = true
			if actor.Kind == "agent" && !agentProjectRestrictionAllows(actor.ProjectIDs, candidate) {
				continue
			}
			if _, allowed := service.authorizeWorkProject(ctx, actor, candidate, actor.OrganizationID, "dependency.read", rid); allowed {
				visibleProjects[candidate] = true
			}
		}
	}
	visibleIDs := make(map[string]bool)
	graph := DependencyGraph{ProjectID: projectID, WorkItems: []WorkItem{}, Dependencies: []WorkItemDependency{}}
	baseRows, err := service.db.QueryContext(ctx, selectWorkItem+` WHERE project_id=$1 AND organization_id=$2 ORDER BY created_at,id`, projectID, actor.OrganizationID)
	if err != nil {
		return serviceUnavailable(rid), nil
	}
	for baseRows.Next() {
		item, err := scanWorkItem(baseRows)
		if err != nil {
			_ = baseRows.Close()
			return serviceUnavailable(rid), nil
		}
		visibleIDs[item.ID] = true
	}
	if err := baseRows.Err(); err != nil {
		_ = baseRows.Close()
		return serviceUnavailable(rid), nil
	}
	if err := baseRows.Close(); err != nil {
		return serviceUnavailable(rid), nil
	}
	for _, edge := range dependencies {
		if !visibleProjects[edge.SourceProject] || !visibleProjects[edge.TargetProject] {
			continue
		}
		graph.Dependencies = append(graph.Dependencies, edge.Dependency)
		visibleIDs[edge.Dependency.SourceWorkItemID], visibleIDs[edge.Dependency.TargetWorkItemID] = true, true
	}
	ids := make([]string, 0, len(visibleIDs))
	for id := range visibleIDs {
		ids = append(ids, id)
	}
	sort.Strings(ids)
	for _, id := range ids {
		item, err := scanWorkItem(service.db.QueryRowContext(ctx, selectWorkItem+` WHERE id=$1 AND organization_id=$2`, id, actor.OrganizationID))
		if err != nil || !visibleProjects[item.ProjectID] {
			return serviceUnavailable(rid), nil
		}
		view, err := workItemView(ctx, service.db, item, nil)
		if err != nil {
			return serviceUnavailable(rid), nil
		}
		graph.WorkItems = append(graph.WorkItems, view)
	}
	return generated.Response{Status: http.StatusOK, Headers: generated.ResponseHeaders{XRequestID: rid}, Body: graph}, nil
}
