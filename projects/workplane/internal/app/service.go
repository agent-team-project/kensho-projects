package app

import (
	"context"
	"database/sql"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"strings"
	"time"
	"unicode/utf8"

	"github.com/agent-team-project/kensho-projects/projects/workplane/internal/api/generated"
	_ "github.com/lib/pq"
)

type Service struct {
	db                *sql.DB
	config            Config
	dummyPasswordHash string
	now               func() time.Time
}

type Actor struct {
	ID             string
	Kind           string
	PrincipalID    *string
	OrganizationID string
	Role           string
	DelegatedRole  string
	Scopes         map[string]bool
	ProjectIDs     map[string]bool
}

type Problem struct {
	Type      string `json:"type"`
	Title     string `json:"title"`
	Status    int    `json:"status"`
	Code      string `json:"code"`
	RequestID string `json:"request_id"`
	Detail    string `json:"detail,omitempty"`
}

type Project struct {
	ID               string   `json:"id"`
	OrganizationID   string   `json:"organization_id"`
	Title            string   `json:"title"`
	Outcome          string   `json:"outcome"`
	Mode             string   `json:"mode"`
	State            string   `json:"state"`
	Version          int64    `json:"version"`
	Hypothesis       string   `json:"hypothesis"`
	Falsifier        string   `json:"falsifier"`
	DecisionCriteria []string `json:"decision_criteria"`
	ExperimentBound  string   `json:"experiment_bound"`
}

type Decision struct {
	ID           string   `json:"id"`
	ProjectID    string   `json:"project_id"`
	ActorID      string   `json:"actor_id"`
	ActorKind    string   `json:"actor_kind"`
	PrincipalID  *string  `json:"principal_id"`
	RecordedAt   string   `json:"recorded_at"`
	Kind         string   `json:"kind"`
	Question     string   `json:"question"`
	Choice       string   `json:"choice"`
	Alternatives []string `json:"alternatives"`
	Rationale    string   `json:"rationale"`
	Evidence     []string `json:"evidence"`
	Consequences []string `json:"consequences"`
}

type Activity struct {
	EventID          string  `json:"event_id"`
	EventType        string  `json:"event_type"`
	ActorID          string  `json:"actor_id"`
	ActorKind        string  `json:"actor_kind"`
	PrincipalID      *string `json:"principal_id"`
	AggregateVersion int64   `json:"aggregate_version"`
	CommandID        string  `json:"command_id"`
	RequestID        string  `json:"request_id"`
	OccurredAt       string  `json:"occurred_at"`
}

func NewService(ctx context.Context, config Config) (*Service, error) {
	db, err := sql.Open("postgres", config.DatabaseURL)
	if err != nil {
		return nil, fmt.Errorf("open database: %w", err)
	}
	db.SetMaxOpenConns(12)
	db.SetMaxIdleConns(4)
	db.SetConnMaxLifetime(30 * time.Minute)
	if err := db.PingContext(ctx); err != nil {
		_ = db.Close()
		return nil, fmt.Errorf("ping database: %w", err)
	}
	dummy, err := hashPassword("constant-shape-invalid-password")
	if err != nil {
		_ = db.Close()
		return nil, err
	}
	service := &Service{db: db, config: config, dummyPasswordHash: dummy, now: func() time.Time { return time.Now().UTC() }}
	if config.Bootstrap != nil {
		if err := service.bootstrap(ctx, *config.Bootstrap); err != nil {
			_ = db.Close()
			return nil, fmt.Errorf("bootstrap walking-slice identities: %w", err)
		}
	}
	return service, nil
}

func (service *Service) Close() error { return service.db.Close() }

func (service *Service) Ready(ctx context.Context) error {
	var version int
	return service.db.QueryRowContext(ctx, "SELECT max(version) FROM schema_migrations").Scan(&version)
}

func (service *Service) bootstrap(ctx context.Context, seed BootstrapConfig) error {
	passwordHash, err := hashPassword(seed.HumanPassword)
	if err != nil {
		return err
	}
	now := service.now()
	tx, err := service.db.BeginTx(ctx, &sql.TxOptions{Isolation: sql.LevelSerializable})
	if err != nil {
		return err
	}
	defer tx.Rollback()
	statements := []struct {
		query string
		args  []any
	}{
		{"INSERT INTO organizations (id, slug, name, created_at) VALUES ($1,$2,$3,$4) ON CONFLICT (id) DO UPDATE SET slug=EXCLUDED.slug,name=EXCLUDED.name", []any{seed.OrganizationID, seed.OrganizationSlug, seed.OrganizationName, now}},
		{"INSERT INTO principals (id, kind, display_name, status, human_principal_id, created_at) VALUES ($1,'human',$2,'active',NULL,$3) ON CONFLICT (id) DO UPDATE SET display_name=EXCLUDED.display_name,status='active'", []any{seed.HumanID, seed.HumanName, now}},
		{"INSERT INTO principals (id, kind, display_name, status, human_principal_id, created_at) VALUES ($1,'agent',$2,'active',$3,$4) ON CONFLICT (id) DO UPDATE SET display_name=EXCLUDED.display_name,status='active',human_principal_id=EXCLUDED.human_principal_id", []any{seed.AgentID, seed.AgentName, seed.HumanID, now}},
		{"INSERT INTO organization_memberships (organization_id, principal_id, role, created_at) VALUES ($1,$2,'owner',$3) ON CONFLICT (organization_id,principal_id) DO UPDATE SET role='owner'", []any{seed.OrganizationID, seed.HumanID, now}},
		{"INSERT INTO organization_memberships (organization_id, principal_id, role, created_at) VALUES ($1,$2,'member',$3) ON CONFLICT (organization_id,principal_id) DO UPDATE SET role='member'", []any{seed.OrganizationID, seed.AgentID, now}},
		{"INSERT INTO human_credentials (principal_id,email,password_hash) VALUES ($1,$2,$3) ON CONFLICT (principal_id) DO UPDATE SET email=EXCLUDED.email,password_hash=EXCLUDED.password_hash", []any{seed.HumanID, seed.HumanEmail, passwordHash}},
	}
	for _, statement := range statements {
		if _, err := tx.ExecContext(ctx, statement.query, statement.args...); err != nil {
			return err
		}
	}
	tokenID := deterministicUUID(seed.AgentID + ":token")
	prefix := tokenPrefix(seed.AgentToken)
	if _, err := tx.ExecContext(ctx, `
		INSERT INTO agent_tokens (id,agent_id,organization_id,token_prefix,token_hash,scopes,project_ids,expires_at,created_by,created_at)
		VALUES ($1,$2,$3,$4,$5,ARRAY[
			'project.create','project.read','project.activate','project.hold','project.resume','project.promote',
			'project.reforecast','project.target.write','project.deadline.write','decision.record',
			'deliverable.read','deliverable.edit','deliverable.reforecast','realtime.subscribe','event.subscribe'
		],NULL,$6,$7,$8)
		ON CONFLICT (id) DO UPDATE SET token_prefix=EXCLUDED.token_prefix,token_hash=EXCLUDED.token_hash,
			scopes=EXCLUDED.scopes,expires_at=EXCLUDED.expires_at,revoked_at=NULL`,
		tokenID, seed.AgentID, seed.OrganizationID, prefix, keyedHash(service.config.TokenHashKey, seed.AgentToken),
		now.Add(24*time.Hour), seed.HumanID, now); err != nil {
		return err
	}
	return tx.Commit()
}

func deterministicUUID(value string) string {
	digest := requestHash([]byte(value))
	digest[6] = (digest[6] & 0x0f) | 0x40
	digest[8] = (digest[8] & 0x3f) | 0x80
	return fmt.Sprintf("%08x-%04x-%04x-%04x-%012x", digest[0:4], digest[4:6], digest[6:8], digest[8:10], digest[10:16])
}

func tokenPrefix(token string) string {
	if len(token) <= 16 {
		return token
	}
	return token[:16]
}

func requestID() string {
	id, err := newUUID()
	if err != nil {
		return fmt.Sprintf("request-%d", time.Now().UnixNano())
	}
	return id
}

func problem(status int, code, title, detail, requestID string) generated.Response {
	return generated.Response{
		Status:      status,
		ContentType: "application/problem+json",
		Headers:     generated.ResponseHeaders{XRequestID: requestID},
		Body: Problem{
			Type: "https://workplane.local/problems/" + code, Title: title,
			Status: status, Code: code, RequestID: requestID, Detail: detail,
		},
	}
}

func decodeStrict[T any](body json.RawMessage) (T, error) {
	var target T
	decoder := json.NewDecoder(strings.NewReader(string(body)))
	decoder.DisallowUnknownFields()
	if err := decoder.Decode(&target); err != nil {
		return target, err
	}
	if decoder.More() {
		return target, errors.New("multiple JSON values")
	}
	var extra any
	if err := decoder.Decode(&extra); !errors.Is(err, io.EOF) {
		if err == nil {
			return target, errors.New("multiple JSON values")
		}
		return target, err
	}
	return target, nil
}

func normalizeStrings(values []string, minItems, maxItems, minLength, maxLength int) ([]string, bool) {
	if values == nil || len(values) < minItems || len(values) > maxItems {
		return nil, false
	}
	result := make([]string, len(values))
	for index, value := range values {
		if utf8.RuneCountInString(value) > maxLength {
			return nil, false
		}
		value = strings.TrimSpace(value)
		if utf8.RuneCountInString(value) < minLength {
			return nil, false
		}
		result[index] = value
	}
	return result, true
}

func validText(value string, min, max int) bool {
	return utf8.RuneCountInString(value) <= max && utf8.RuneCountInString(strings.TrimSpace(value)) >= min
}

func (service *Service) audit(ctx context.Context, category, requestID string, actor *Actor, detail map[string]any) {
	eventID, err := newUUID()
	if err != nil {
		return
	}
	encoded, _ := json.Marshal(detail)
	var actorID, orgID any
	if actor != nil {
		if actor.ID != "" {
			actorID = actor.ID
		}
		if actor.OrganizationID != "" {
			orgID = actor.OrganizationID
		}
	}
	_, _ = service.db.ExecContext(ctx, `INSERT INTO security_audit_events
		(event_id,category,actor_id,organization_id,request_id,occurred_at,detail)
		VALUES ($1,$2,$3,$4,$5,$6,$7)`, eventID, category, actorID, orgID, requestID, service.now(), encoded)
}

func (service *Service) authenticate(ctx context.Context, request generated.Request, action, projectID string) (Actor, generated.Response, bool) {
	return service.authenticateActor(ctx, request, action, projectID, false)
}

func (service *Service) authenticateSubscription(ctx context.Context, request generated.Request, action string) (Actor, generated.Response, bool) {
	return service.authenticateActor(ctx, request, action, "", true)
}

func (service *Service) authenticateActor(ctx context.Context, request generated.Request, action, projectID string, subscription bool) (Actor, generated.Response, bool) {
	rid := requestID()
	hasSession := request.Security.SessionCookie != ""
	hasBearer := request.Security.BearerToken != ""
	hasAuthorization := request.HTTPRequest.Header.Get("Authorization") != ""
	if hasSession == hasBearer || (hasSession && hasAuthorization) || (hasAuthorization && !hasBearer) {
		return Actor{}, problem(http.StatusUnauthorized, "unauthenticated", "Authentication required", "Use exactly one authentication transport.", rid), false
	}
	if hasSession {
		hash := keyedHash(service.config.TokenHashKey, request.Security.SessionCookie)
		var actor Actor
		var status string
		err := service.db.QueryRowContext(ctx, `
			SELECT p.id,p.kind,p.status,m.organization_id,m.role
			FROM human_sessions s JOIN principals p ON p.id=s.principal_id
			JOIN organization_memberships m ON m.principal_id=p.id
			WHERE s.token_hash=$1 AND s.revoked_at IS NULL AND s.expires_at > CURRENT_TIMESTAMP
			ORDER BY m.organization_id LIMIT 1`, hash).Scan(&actor.ID, &actor.Kind, &status, &actor.OrganizationID, &actor.Role)
		if err != nil || status != "active" || actor.Kind != "human" {
			return Actor{}, problem(http.StatusUnauthorized, "unauthenticated", "Authentication required", "The session is invalid or expired.", rid), false
		}
		actor.Scopes = map[string]bool{action: true}
		return actor, generated.Response{Headers: generated.ResponseHeaders{XRequestID: rid}}, true
	}
	token := request.Security.BearerToken
	prefix := tokenPrefix(token)
	var actor Actor
	var actorStatus, principalStatus string
	var tokenHash []byte
	var scopesJSON, projectIDsJSON string
	err := service.db.QueryRowContext(ctx, `
		SELECT a.id,a.kind,a.status,a.human_principal_id,h.status,t.organization_id,m.role,hm.role,t.token_hash,
			array_to_json(t.scopes)::text,array_to_json(COALESCE(t.project_ids,ARRAY[]::uuid[]))::text
		FROM agent_tokens t JOIN principals a ON a.id=t.agent_id
		JOIN principals h ON h.id=a.human_principal_id
		JOIN organization_memberships m ON m.organization_id=t.organization_id AND m.principal_id=a.id
		JOIN organization_memberships hm ON hm.organization_id=t.organization_id AND hm.principal_id=h.id
		WHERE t.token_prefix=$1 AND t.revoked_at IS NULL AND t.expires_at > CURRENT_TIMESTAMP`, prefix).Scan(
		&actor.ID, &actor.Kind, &actorStatus, &actor.PrincipalID, &principalStatus,
		&actor.OrganizationID, &actor.Role, &actor.DelegatedRole, &tokenHash, &scopesJSON, &projectIDsJSON)
	var scopes, projectIDs []string
	if err == nil {
		err = json.Unmarshal([]byte(scopesJSON), &scopes)
	}
	if err == nil {
		err = json.Unmarshal([]byte(projectIDsJSON), &projectIDs)
	}
	if err != nil || !hashesEqual(tokenHash, keyedHash(service.config.TokenHashKey, token)) ||
		actorStatus != "active" || principalStatus != "active" || actor.Kind != "agent" || actor.PrincipalID == nil || *actor.PrincipalID == actor.ID {
		return Actor{}, problem(http.StatusUnauthorized, "unauthenticated", "Authentication required", "The agent token is invalid or expired.", rid), false
	}
	actor.Scopes = make(map[string]bool, len(scopes))
	for _, scope := range scopes {
		actor.Scopes[scope] = true
	}
	actor.ProjectIDs = make(map[string]bool, len(projectIDs))
	for _, id := range projectIDs {
		actor.ProjectIDs[id] = true
	}
	denialReason := ""
	if !actor.Scopes[action] {
		denialReason = "agent_action_scope"
	} else if !subscription && !agentProjectRestrictionAllows(actor.ProjectIDs, projectID) {
		denialReason = "agent_project_restriction"
	}
	if denialReason != "" {
		service.audit(ctx, "authorization.denied", rid, &actor, map[string]any{"reason": denialReason, "action": action})
		return Actor{}, problem(http.StatusForbidden, "forbidden", "Action denied", "The delegated token does not include this action.", rid), false
	}
	_, _ = service.db.ExecContext(ctx, "UPDATE agent_tokens SET last_used_at=CURRENT_TIMESTAMP WHERE token_prefix=$1", prefix)
	return actor, generated.Response{Headers: generated.ResponseHeaders{XRequestID: rid}}, true
}

func (service *Service) authorizeOrganization(actor Actor, organizationID, action, rid string) (generated.Response, bool) {
	if actor.OrganizationID != organizationID {
		return problem(http.StatusNotFound, "not_found", "Resource not found", "The requested resource is not available.", rid), false
	}
	if !organizationRoleAllows(actor.Role, action) || (actor.Kind == "agent" && !organizationRoleAllows(actor.DelegatedRole, action)) {
		return problem(http.StatusForbidden, "forbidden", "Action denied", "The active role does not permit this action.", rid), false
	}
	return generated.Response{}, true
}

// agentProjectRestrictionAllows implements the token's fail-closed project
// allowlist. An empty allowlist is organization-scoped. A nonempty allowlist is
// project-scoped and therefore cannot authorize an operation, such as project
// creation, that has no existing project target.
func agentProjectRestrictionAllows(projectIDs map[string]bool, projectID string) bool {
	if len(projectIDs) == 0 {
		return true
	}
	return projectID != "" && projectIDs[projectID]
}

func organizationRoleAllows(role, action string) bool {
	return role == "owner" || role == "admin" || role == "member" ||
		(role == "observer" && (action == "project.read" || action == "deliverable.read" ||
			action == "realtime.subscribe" || action == "event.subscribe"))
}

func organizationVisibilityAllows(action string) bool {
	return action == "project.read" || action == "deliverable.read"
}

func (service *Service) authorizeProject(ctx context.Context, actor Actor, projectID, organizationID, action, rid string) (generated.Response, bool) {
	if denied, ok := service.authorizeOrganization(actor, organizationID, action, rid); !ok {
		return denied, false
	}
	var visibility, createdBy string
	var participant bool
	principalID := ""
	if actor.PrincipalID != nil {
		principalID = *actor.PrincipalID
	}
	err := service.db.QueryRowContext(ctx, `SELECT visibility::text,created_by,
		EXISTS (SELECT 1 FROM project_memberships membership WHERE membership.project_id=projects.id
			AND (membership.principal_id=$3 OR membership.principal_id=NULLIF($4,'')::uuid))
		FROM projects WHERE id=$1 AND organization_id=$2`, projectID, organizationID, actor.ID, principalID).
		Scan(&visibility, &createdBy, &participant)
	if err != nil {
		return problem(http.StatusNotFound, "not_found", "Resource not found", "The requested resource is not available.", rid), false
	}
	if createdBy == actor.ID || (principalID != "" && createdBy == principalID) || participant ||
		(privilegedProjectRole(actor.Role) && (actor.Kind != "agent" || privilegedProjectRole(actor.DelegatedRole))) {
		return generated.Response{}, true
	}
	if visibility == "organization" {
		if organizationVisibilityAllows(action) {
			return generated.Response{}, true
		}
		return problem(http.StatusForbidden, "forbidden", "Action denied", "A project role is required for this action.", rid), false
	}
	return problem(http.StatusNotFound, "not_found", "Resource not found", "The requested resource is not available.", rid), false
}

func (service *Service) requireHumanMutation(request generated.Request, actor Actor, rid string) (generated.Response, bool) {
	if actor.Kind != "human" {
		if request.Security.CSRFToken != "" || request.Security.SessionCookie != "" {
			return problem(http.StatusBadRequest, "invalid_request", "Invalid authentication transport", "Agent requests cannot use browser credentials.", rid), false
		}
		return generated.Response{}, true
	}
	if request.Security.CSRFToken == "" || request.HTTPRequest.Header.Get("Origin") != service.config.PublicOrigin {
		return problem(http.StatusForbidden, "forbidden", "Browser mutation denied", "A valid origin and CSRF token are required.", rid), false
	}
	var expected []byte
	err := service.db.QueryRowContext(request.HTTPRequest.Context(), `
		SELECT csrf_hash FROM human_sessions WHERE token_hash=$1 AND revoked_at IS NULL AND expires_at>CURRENT_TIMESTAMP`,
		keyedHash(service.config.TokenHashKey, request.Security.SessionCookie)).Scan(&expected)
	if err != nil || !hashesEqual(expected, keyedHash(service.config.TokenHashKey, request.Security.CSRFToken)) {
		return problem(http.StatusForbidden, "forbidden", "Browser mutation denied", "A valid origin and CSRF token are required.", rid), false
	}
	return generated.Response{}, true
}

func canonicalJSON(value any) []byte {
	encoded, _ := json.Marshal(value)
	return encoded
}

func scanProject(row interface{ Scan(...any) error }) (Project, error) {
	var project Project
	var criteria []byte
	err := row.Scan(&project.ID, &project.OrganizationID, &project.Title, &project.Outcome,
		&project.Mode, &project.State, &project.Version, &project.Hypothesis,
		&project.Falsifier, &criteria, &project.ExperimentBound)
	if err != nil {
		return Project{}, err
	}
	if err := json.Unmarshal(criteria, &project.DecisionCriteria); err != nil {
		return Project{}, err
	}
	return project, nil
}

const selectProject = `SELECT id,organization_id,title,outcome,mode,state,version,hypothesis,falsifier,decision_criteria,experiment_bound FROM projects`
