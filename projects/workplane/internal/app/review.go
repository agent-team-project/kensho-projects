package app

import (
	"context"
	"database/sql"
	"encoding/json"
	"errors"
	"net/http"
	"sort"
	"strings"
	"time"

	"github.com/agent-team-project/kensho-projects/projects/workplane/internal/api/generated"
	"github.com/lib/pq"
)

type EvidenceSupport struct {
	TargetType string `json:"target_type"`
	TargetID   string `json:"target_id"`
}

type Evidence struct {
	ID              string            `json:"id"`
	OrganizationID  string            `json:"organization_id"`
	ProjectID       string            `json:"project_id"`
	Kind            string            `json:"kind"`
	Title           string            `json:"title"`
	Claim           string            `json:"claim"`
	Source          string            `json:"source"`
	Content         *string           `json:"content"`
	URI             *string           `json:"uri"`
	Metadata        map[string]string `json:"metadata"`
	IntegrityDigest string            `json:"integrity_digest"`
	ProducedBy      string            `json:"produced_by"`
	ProducerKind    string            `json:"producer_kind"`
	PrincipalID     *string           `json:"principal_id"`
	ProducedAt      string            `json:"produced_at"`
	SupersedesID    *string           `json:"supersedes_id"`
	Supports        []EvidenceSupport `json:"supports"`
}

type EvidenceRequirement struct {
	Kind  string `json:"kind"`
	Claim string `json:"claim"`
}

type Gate struct {
	ID                   string                `json:"id"`
	OrganizationID       string                `json:"organization_id"`
	ProjectID            string                `json:"project_id"`
	DeliverableID        string                `json:"deliverable_id"`
	Name                 string                `json:"name"`
	Kind                 string                `json:"kind"`
	Hard                 bool                  `json:"hard"`
	IndependenceRequired bool                  `json:"independence_required"`
	State                string                `json:"state"`
	Version              int64                 `json:"version"`
	WaiverDecisionID     *string               `json:"waiver_decision_id"`
	RequiredEvidence     []EvidenceRequirement `json:"required_evidence"`
	CreatedBy            string                `json:"created_by"`
	CreatedAt            string                `json:"created_at"`
	UpdatedAt            string                `json:"updated_at"`
}

type Verdict struct {
	ID             string   `json:"id"`
	OrganizationID string   `json:"organization_id"`
	ProjectID      string   `json:"project_id"`
	DeliverableID  string   `json:"deliverable_id"`
	GateID         string   `json:"gate_id"`
	Result         string   `json:"result"`
	EvidenceIDs    []string `json:"evidence_ids"`
	ReviewerID     string   `json:"reviewer_id"`
	ReviewerKind   string   `json:"reviewer_kind"`
	PrincipalID    *string  `json:"principal_id"`
	SupersedesID   *string  `json:"supersedes_id"`
	CreatedAt      string   `json:"created_at"`
}

type Finding struct {
	ID             string `json:"id"`
	OrganizationID string `json:"organization_id"`
	ProjectID      string `json:"project_id"`
	DeliverableID  string `json:"deliverable_id"`
	GateID         string `json:"gate_id"`
	VerdictID      string `json:"verdict_id"`
	Title          string `json:"title"`
	Detail         string `json:"detail"`
	Blocking       bool   `json:"blocking"`
	State          string `json:"state"`
	Version        int64  `json:"version"`
	CreatedBy      string `json:"created_by"`
	CreatedAt      string `json:"created_at"`
	UpdatedAt      string `json:"updated_at"`
}

type FindingAction struct {
	ID             string   `json:"id"`
	OrganizationID string   `json:"organization_id"`
	ProjectID      string   `json:"project_id"`
	DeliverableID  string   `json:"deliverable_id"`
	FindingID      string   `json:"finding_id"`
	Action         string   `json:"action"`
	EvidenceIDs    []string `json:"evidence_ids"`
	Rationale      string   `json:"rationale"`
	ActorID        string   `json:"actor_id"`
	ActorKind      string   `json:"actor_kind"`
	PrincipalID    *string  `json:"principal_id"`
	CreatedAt      string   `json:"created_at"`
}

type Submission struct {
	ID             string   `json:"id"`
	OrganizationID string   `json:"organization_id"`
	ProjectID      string   `json:"project_id"`
	DeliverableID  string   `json:"deliverable_id"`
	Kind           string   `json:"kind"`
	EvidenceIDs    []string `json:"evidence_ids"`
	Note           string   `json:"note"`
	SubmittedBy    string   `json:"submitted_by"`
	SubmitterKind  string   `json:"submitter_kind"`
	PrincipalID    *string  `json:"principal_id"`
	CreatedAt      string   `json:"created_at"`
}

type SubmissionHead struct {
	DeliverableID string `json:"deliverable_id"`
	SubmissionID  string `json:"submission_id"`
}

type VerdictResult struct {
	Gate     Gate      `json:"gate"`
	Verdict  Verdict   `json:"verdict"`
	Findings []Finding `json:"findings"`
}

type FindingActionResult struct {
	Finding Finding       `json:"finding"`
	Action  FindingAction `json:"action"`
}

type SubmissionResult struct {
	Deliverable Deliverable `json:"deliverable"`
	Submission  Submission  `json:"submission"`
}

type DeliverableReviewEvent struct {
	Deliverable Deliverable `json:"deliverable"`
	FindingID   *string     `json:"finding_id"`
	Rationale   string      `json:"rationale"`
}

type DeliverableWaiverResult struct {
	Deliverable Deliverable `json:"deliverable"`
	Decision    Decision    `json:"decision"`
}

type GateWaiverResult struct {
	Gate     Gate     `json:"gate"`
	Decision Decision `json:"decision"`
}

type GateVerdictEvent struct {
	Gate    Gate    `json:"gate"`
	Verdict Verdict `json:"verdict"`
}

const selectEvidence = `SELECT id,organization_id,project_id,kind::text,title,claim,source,content,uri,metadata,
	encode(integrity_digest,'hex'),produced_by,producer_kind::text,principal_id,produced_at,supersedes_id FROM evidence`

const selectGate = `SELECT id,organization_id,project_id,deliverable_id,name,kind::text,hard,independence_required,
	state::text,version,waiver_decision_id,created_by,created_at,updated_at FROM review_gates`

const selectVerdict = `SELECT id,organization_id,project_id,deliverable_id,gate_id,result::text,evidence_ids,
	reviewer_id,reviewer_kind::text,principal_id,supersedes_id,created_at FROM review_verdicts`

const selectFinding = `SELECT id,organization_id,project_id,deliverable_id,gate_id,verdict_id,title,detail,
	blocking,state::text,version,created_by,created_at,updated_at FROM review_findings`

func appendReviewEvent(ctx context.Context, tx *sql.Tx, service *Service, request generated.Request, actor Actor, rid string,
	project *Project, eventType string, payload any, now time.Time, boundary string) (string, error) {
	project.Version++
	if _, err := tx.ExecContext(ctx, `UPDATE projects SET version=$1,updated_at=$2 WHERE id=$3`, project.Version, now, project.ID); err != nil {
		return "", err
	}
	return insertPlanningEvent(ctx, tx, service, request, actor, rid, project.ID, project.Version, eventType, payload, now, boundary)
}

func normalizeUUIDList(values []string, minItems, maxItems int) ([]string, bool) {
	if len(values) < minItems || len(values) > maxItems {
		return nil, false
	}
	seen := make(map[string]bool, len(values))
	result := make([]string, len(values))
	for index, value := range values {
		value = strings.ToLower(strings.TrimSpace(value))
		if !uuidPattern.MatchString(value) || seen[value] {
			return nil, false
		}
		seen[value] = true
		result[index] = value
	}
	return result, true
}

func nullableText(value *string, max int) (*string, bool) {
	if value == nil {
		return nil, true
	}
	normalized := strings.TrimSpace(*value)
	if !validText(normalized, 1, max) {
		return nil, false
	}
	return &normalized, true
}

func normalizeEvidenceInput(input generated.EvidenceInput) (generated.EvidenceInput, bool) {
	input.Title, input.Claim, input.Source = strings.TrimSpace(input.Title), strings.TrimSpace(input.Claim), strings.TrimSpace(input.Source)
	validKinds := map[string]bool{"report": true, "test-run": true, "link": true, "image": true, "observation": true, "measurement": true}
	if !validKinds[input.Kind] || !validText(input.Title, 1, 200) || !validText(input.Claim, 1, 2000) ||
		!validText(input.Source, 1, 2000) || len(input.Metadata) > 32 || len(input.Supports) < 1 || len(input.Supports) > 32 {
		return generated.EvidenceInput{}, false
	}
	var ok bool
	if input.Content, ok = nullableText(input.Content, 16000); !ok {
		return generated.EvidenceInput{}, false
	}
	if input.URI, ok = nullableText(input.URI, 2000); !ok {
		return generated.EvidenceInput{}, false
	}
	if input.Content == nil && input.URI == nil {
		return generated.EvidenceInput{}, false
	}
	metadata := make(map[string]string, len(input.Metadata))
	for key, value := range input.Metadata {
		key, value = strings.TrimSpace(key), strings.TrimSpace(value)
		if !validText(key, 1, 100) || !validText(value, 1, 2000) {
			return generated.EvidenceInput{}, false
		}
		metadata[key] = value
	}
	input.Metadata = metadata
	seen := make(map[string]bool, len(input.Supports))
	for index := range input.Supports {
		item := &input.Supports[index]
		item.TargetType, item.TargetID = strings.TrimSpace(item.TargetType), strings.ToLower(strings.TrimSpace(item.TargetID))
		if !map[string]bool{"deliverable": true, "gate": true, "finding": true, "decision": true}[item.TargetType] ||
			!uuidPattern.MatchString(item.TargetID) || seen[item.TargetType+":"+item.TargetID] {
			return generated.EvidenceInput{}, false
		}
		seen[item.TargetType+":"+item.TargetID] = true
	}
	sort.Slice(input.Supports, func(i, j int) bool {
		return input.Supports[i].TargetType+input.Supports[i].TargetID < input.Supports[j].TargetType+input.Supports[j].TargetID
	})
	return input, true
}

func (service *Service) projectIDForEvidence(ctx context.Context, id string) (string, bool) {
	if !uuidPattern.MatchString(id) {
		return "", false
	}
	var projectID string
	return projectID, service.db.QueryRowContext(ctx, `SELECT project_id FROM evidence WHERE id=$1`, id).Scan(&projectID) == nil
}

func (service *Service) projectIDForGate(ctx context.Context, id string) (string, bool) {
	if !uuidPattern.MatchString(id) {
		return "", false
	}
	var projectID string
	return projectID, service.db.QueryRowContext(ctx, `SELECT project_id FROM review_gates WHERE id=$1`, id).Scan(&projectID) == nil
}

func (service *Service) projectIDForFinding(ctx context.Context, id string) (string, bool) {
	if !uuidPattern.MatchString(id) {
		return "", false
	}
	var projectID string
	return projectID, service.db.QueryRowContext(ctx, `SELECT project_id FROM review_findings WHERE id=$1`, id).Scan(&projectID) == nil
}

func scanEvidence(row interface{ Scan(...any) error }) (Evidence, error) {
	var item Evidence
	var content, uri, principal, supersedes sql.NullString
	var metadata []byte
	var produced time.Time
	if err := row.Scan(&item.ID, &item.OrganizationID, &item.ProjectID, &item.Kind, &item.Title, &item.Claim, &item.Source,
		&content, &uri, &metadata, &item.IntegrityDigest, &item.ProducedBy, &item.ProducerKind, &principal, &produced, &supersedes); err != nil {
		return Evidence{}, err
	}
	if content.Valid {
		item.Content = &content.String
	}
	if uri.Valid {
		item.URI = &uri.String
	}
	if principal.Valid {
		item.PrincipalID = &principal.String
	}
	if supersedes.Valid {
		item.SupersedesID = &supersedes.String
	}
	if err := json.Unmarshal(metadata, &item.Metadata); err != nil {
		return Evidence{}, err
	}
	item.ProducedAt = produced.UTC().Format(timeFormat)
	return item, nil
}

func loadEvidenceSupports(ctx context.Context, queryer databaseQueryer, item *Evidence) error {
	rows, err := queryer.QueryContext(ctx, `SELECT target_kind::text,target_id FROM evidence_links WHERE evidence_id=$1 ORDER BY target_kind,target_id`, item.ID)
	if err != nil {
		return err
	}
	defer rows.Close()
	item.Supports = make([]EvidenceSupport, 0)
	for rows.Next() {
		var support EvidenceSupport
		if err := rows.Scan(&support.TargetType, &support.TargetID); err != nil {
			return err
		}
		item.Supports = append(item.Supports, support)
	}
	return rows.Err()
}

func evidenceTargetExists(ctx context.Context, tx *sql.Tx, projectID string, support generated.EvidenceSupportInput) bool {
	queries := map[string]string{
		"deliverable": `SELECT EXISTS (SELECT 1 FROM deliverables WHERE id=$1 AND project_id=$2)`,
		"gate":        `SELECT EXISTS (SELECT 1 FROM review_gates WHERE id=$1 AND project_id=$2)`,
		"finding":     `SELECT EXISTS (SELECT 1 FROM review_findings WHERE id=$1 AND project_id=$2)`,
		"decision":    `SELECT EXISTS (SELECT 1 FROM decisions WHERE id=$1 AND project_id=$2)`,
	}
	var exists bool
	return tx.QueryRowContext(ctx, queries[support.TargetType], support.TargetID, projectID).Scan(&exists) == nil && exists
}

func (service *Service) createEvidenceMutation(ctx context.Context, request generated.Request, actor Actor, rid, projectID, operation string,
	expected int64, input generated.EvidenceInput, supersedesID *string) generated.Response {
	return service.executeProjectMutation(ctx, request, actor, rid, projectID, operation, expected, canonicalJSON(input),
		func(tx *sql.Tx, project Project, actor Actor, rid string, now time.Time) (mutationOutcome, *generated.Response, error) {
			if supersedesID != nil {
				var existingProject string
				// The project row is already locked by executeProjectMutation, which
				// serializes every supersession attempt without granting UPDATE on
				// the append-only evidence table merely to obtain a row lock.
				if err := tx.QueryRowContext(ctx, `SELECT project_id FROM evidence WHERE id=$1`, *supersedesID).Scan(&existingProject); err != nil || existingProject != project.ID {
					return mutationOutcome{}, rejected(problem(http.StatusNotFound, "not_found", "Resource not found", "The requested resource is not available.", rid)), nil
				}
				var already bool
				if err := tx.QueryRowContext(ctx, `SELECT EXISTS (SELECT 1 FROM evidence WHERE supersedes_id=$1)`, *supersedesID).Scan(&already); err != nil {
					return mutationOutcome{}, nil, err
				}
				if already {
					return mutationOutcome{}, rejected(problem(http.StatusConflict, "invariant_violation", "Evidence already corrected", "Supersession chains cannot branch or rewrite history.", rid)), nil
				}
			}
			for _, support := range input.Supports {
				if !evidenceTargetExists(ctx, tx, project.ID, support) {
					return mutationOutcome{}, rejected(problem(http.StatusConflict, "invariant_violation", "Evidence target invalid", "Every evidence link must target this project.", rid)), nil
				}
			}
			id, err := newUUID()
			if err != nil {
				return mutationOutcome{}, nil, err
			}
			material := map[string]any{"kind": input.Kind, "title": input.Title, "claim": input.Claim, "source": input.Source,
				"content": input.Content, "uri": input.URI, "metadata": input.Metadata, "supports": input.Supports, "supersedes_id": supersedesID}
			var digest string
			if err := tx.QueryRowContext(ctx, `INSERT INTO evidence
				(id,organization_id,project_id,kind,title,claim,source,content,uri,metadata,integrity_material,integrity_digest,
				 produced_by,producer_kind,principal_id,produced_at,supersedes_id)
				VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,digest(convert_to($11::jsonb::text,'UTF8'),'sha256'),$12,$13,$14,$15,$16)
				RETURNING encode(integrity_digest,'hex')`, id, actor.OrganizationID, project.ID, input.Kind, input.Title, input.Claim,
				input.Source, input.Content, input.URI, canonicalJSON(input.Metadata), canonicalJSON(material), actor.ID, actor.Kind,
				actor.PrincipalID, now, supersedesID).Scan(&digest); err != nil {
				return mutationOutcome{}, nil, err
			}
			for _, support := range input.Supports {
				if _, err := tx.ExecContext(ctx, `INSERT INTO evidence_links (project_id,evidence_id,target_kind,target_id) VALUES ($1,$2,$3,$4)`,
					project.ID, id, support.TargetType, support.TargetID); err != nil {
					return mutationOutcome{}, nil, err
				}
			}
			if service.config.FaultInjection && request.HTTPRequest.Header.Get("X-Workplane-Fault") == "after-evidence" {
				return mutationOutcome{}, nil, ErrInjectedCrash
			}
			item := Evidence{ID: id, OrganizationID: actor.OrganizationID, ProjectID: project.ID, Kind: input.Kind, Title: input.Title,
				Claim: input.Claim, Source: input.Source, Content: input.Content, URI: input.URI, Metadata: input.Metadata,
				IntegrityDigest: digest, ProducedBy: actor.ID, ProducerKind: actor.Kind, PrincipalID: actor.PrincipalID,
				ProducedAt: now.Format(timeFormat), SupersedesID: supersedesID, Supports: make([]EvidenceSupport, len(input.Supports))}
			for index, support := range input.Supports {
				item.Supports[index] = EvidenceSupport{TargetType: support.TargetType, TargetID: support.TargetID}
			}
			eventType := "evidence.created"
			if supersedesID != nil {
				eventType = "evidence.superseded"
			}
			eventID, err := appendReviewEvent(ctx, tx, service, request, actor, rid, &project, eventType, item, now, "after-evidence-event")
			if err != nil {
				return mutationOutcome{}, nil, err
			}
			return mutationOutcome{Status: http.StatusCreated, Body: item, Version: project.Version, EventIDs: []string{eventID}}, nil, nil
		})
}

func (service *Service) CreateEvidence(ctx context.Context, request generated.Request) (generated.Response, error) {
	projectID := request.HTTPRequest.PathValue("project_id")
	actor, rid, expected, response, ok := service.prepareProjectMutation(ctx, request, []string{"evidence.create"}, projectID)
	if !ok {
		return response, nil
	}
	input, err := decodeStrict[generated.EvidenceInput](request.Body)
	input, valid := normalizeEvidenceInput(input)
	if err != nil || !valid {
		return problem(http.StatusBadRequest, "invalid_request", "Invalid evidence", "Evidence requires declared provenance, canonical content or link metadata, and project-scoped support links.", rid), nil
	}
	return service.createEvidenceMutation(ctx, request, actor, rid, projectID, "createEvidence", expected, input, nil), nil
}

func (service *Service) SupersedeEvidence(ctx context.Context, request generated.Request) (generated.Response, error) {
	id := request.HTTPRequest.PathValue("id")
	projectID, found := service.projectIDForEvidence(ctx, id)
	if !found {
		projectID = "00000000-0000-4000-8000-000000000000"
	}
	actor, rid, expected, response, ok := service.prepareProjectMutation(ctx, request, []string{"evidence.supersede"}, projectID)
	if !ok {
		return response, nil
	}
	input, err := decodeStrict[generated.EvidenceInput](request.Body)
	input, valid := normalizeEvidenceInput(input)
	if err != nil || !valid {
		return problem(http.StatusBadRequest, "invalid_request", "Invalid evidence correction", "A correction must be a complete new attributable evidence record.", rid), nil
	}
	return service.createEvidenceMutation(ctx, request, actor, rid, projectID, "supersedeEvidence", expected, input, &id), nil
}

func (service *Service) ListEvidence(ctx context.Context, request generated.Request) (generated.Response, error) {
	projectID := request.HTTPRequest.PathValue("project_id")
	actor, authResponse, ok := service.authenticate(ctx, request, "evidence.read", projectID)
	if !ok {
		return authResponse, nil
	}
	rid := authResponse.Headers.XRequestID
	if len(strings.TrimSpace(string(request.Body))) > 0 || !uuidPattern.MatchString(projectID) {
		return problem(http.StatusNotFound, "not_found", "Resource not found", "The requested resource is not available.", rid), nil
	}
	if denied, allowed := service.authorizeProject(ctx, actor, projectID, actor.OrganizationID, "evidence.read", rid); !allowed {
		return denied, nil
	}
	rows, err := service.db.QueryContext(ctx, selectEvidence+` WHERE project_id=$1 AND organization_id=$2 ORDER BY produced_at,id`, projectID, actor.OrganizationID)
	if err != nil {
		return serviceUnavailable(rid), nil
	}
	defer rows.Close()
	items := make([]Evidence, 0)
	for rows.Next() {
		item, err := scanEvidence(rows)
		if err != nil {
			return serviceUnavailable(rid), nil
		}
		if err := loadEvidenceSupports(ctx, service.db, &item); err != nil {
			return serviceUnavailable(rid), nil
		}
		items = append(items, item)
	}
	if err := rows.Err(); err != nil {
		return serviceUnavailable(rid), nil
	}
	return generated.Response{Status: http.StatusOK, Headers: generated.ResponseHeaders{XRequestID: rid}, Body: items}, nil
}

func normalizeGateInput(input generated.GateInput) (generated.GateInput, bool) {
	input.Name = strings.TrimSpace(input.Name)
	if !validText(input.Name, 1, 200) || !map[string]bool{"automated": true, "review": true, "approval": true}[input.Kind] ||
		input.Hard == nil || input.IndependenceRequired == nil || len(input.RequiredEvidence) < 1 || len(input.RequiredEvidence) > 32 {
		return generated.GateInput{}, false
	}
	seen := make(map[string]bool, len(input.RequiredEvidence))
	for index := range input.RequiredEvidence {
		item := &input.RequiredEvidence[index]
		item.Claim = strings.TrimSpace(item.Claim)
		if !map[string]bool{"report": true, "test-run": true, "link": true, "image": true, "observation": true, "measurement": true}[item.Kind] ||
			!validText(item.Claim, 1, 2000) || seen[item.Kind+":"+item.Claim] {
			return generated.GateInput{}, false
		}
		seen[item.Kind+":"+item.Claim] = true
	}
	return input, true
}

func scanGate(row interface{ Scan(...any) error }) (Gate, error) {
	var item Gate
	var waiver sql.NullString
	var created, updated time.Time
	if err := row.Scan(&item.ID, &item.OrganizationID, &item.ProjectID, &item.DeliverableID, &item.Name, &item.Kind,
		&item.Hard, &item.IndependenceRequired, &item.State, &item.Version, &waiver, &item.CreatedBy, &created, &updated); err != nil {
		return Gate{}, err
	}
	if waiver.Valid {
		item.WaiverDecisionID = &waiver.String
	}
	item.CreatedAt, item.UpdatedAt = created.UTC().Format(timeFormat), updated.UTC().Format(timeFormat)
	return item, nil
}

func loadGateRequirements(ctx context.Context, queryer databaseQueryer, item *Gate) error {
	rows, err := queryer.QueryContext(ctx, `SELECT kind::text,claim FROM gate_evidence_requirements WHERE gate_id=$1 ORDER BY ordinal`, item.ID)
	if err != nil {
		return err
	}
	defer rows.Close()
	item.RequiredEvidence = make([]EvidenceRequirement, 0)
	for rows.Next() {
		var requirement EvidenceRequirement
		if err := rows.Scan(&requirement.Kind, &requirement.Claim); err != nil {
			return err
		}
		item.RequiredEvidence = append(item.RequiredEvidence, requirement)
	}
	return rows.Err()
}

func loadGate(ctx context.Context, queryer databaseQueryer, id string, lock bool) (Gate, error) {
	query := selectGate + ` WHERE id=$1`
	if lock {
		query += ` FOR UPDATE`
	}
	item, err := scanGate(queryer.QueryRowContext(ctx, query, id))
	if err != nil {
		return Gate{}, err
	}
	if err := loadGateRequirements(ctx, queryer, &item); err != nil {
		return Gate{}, err
	}
	return item, nil
}

func (service *Service) CreateGate(ctx context.Context, request generated.Request) (generated.Response, error) {
	id := request.HTTPRequest.PathValue("id")
	projectID, found := service.deliverableProjectID(ctx, id)
	if !found {
		projectID = "00000000-0000-4000-8000-000000000000"
	}
	actor, rid, expected, response, ok := service.prepareProjectMutation(ctx, request, []string{"deliverable.edit"}, projectID)
	if !ok {
		return response, nil
	}
	input, err := decodeStrict[generated.GateInput](request.Body)
	input, valid := normalizeGateInput(input)
	if err != nil || !valid {
		return problem(http.StatusBadRequest, "invalid_request", "Invalid gate", "A gate requires stable kind, hardness, independence, and a non-empty evidence contract.", rid), nil
	}
	result := service.executeProjectMutation(ctx, request, actor, rid, projectID, "createGate", expected, canonicalJSON(input),
		func(tx *sql.Tx, project Project, actor Actor, rid string, now time.Time) (mutationOutcome, *generated.Response, error) {
			deliverable, err := scanDeliverable(tx.QueryRowContext(ctx, selectDeliverable+` WHERE id=$1 AND project_id=$2 FOR UPDATE`, id, project.ID))
			if err != nil {
				return mutationOutcome{}, rejected(problem(http.StatusNotFound, "not_found", "Resource not found", "The requested resource is not available.", rid)), nil
			}
			if project.Mode != "exploitation" || !mutableProject(project) || (deliverable.State != "draft" && deliverable.State != "ready") {
				return mutationOutcome{}, rejected(problem(http.StatusConflict, "invariant_violation", "Gate not allowed", "Gates are frozen when a deliverable enters review.", rid)), nil
			}
			gateID, err := newUUID()
			if err != nil {
				return mutationOutcome{}, nil, err
			}
			gate := Gate{ID: gateID, OrganizationID: actor.OrganizationID, ProjectID: project.ID, DeliverableID: id, Name: input.Name,
				Kind: input.Kind, Hard: *input.Hard, IndependenceRequired: *input.IndependenceRequired, State: "pending", Version: 1,
				CreatedBy: actor.ID, CreatedAt: now.Format(timeFormat), UpdatedAt: now.Format(timeFormat),
				RequiredEvidence: make([]EvidenceRequirement, len(input.RequiredEvidence))}
			if _, err := tx.ExecContext(ctx, `INSERT INTO review_gates
				(id,organization_id,project_id,deliverable_id,name,kind,hard,independence_required,state,version,created_by,created_at,updated_at)
				VALUES ($1,$2,$3,$4,$5,$6,$7,$8,'pending',1,$9,$10,$10)`, gate.ID, gate.OrganizationID, gate.ProjectID,
				gate.DeliverableID, gate.Name, gate.Kind, gate.Hard, gate.IndependenceRequired, gate.CreatedBy, now); err != nil {
				return mutationOutcome{}, nil, err
			}
			for index, requirement := range input.RequiredEvidence {
				gate.RequiredEvidence[index] = EvidenceRequirement{Kind: requirement.Kind, Claim: requirement.Claim}
				if _, err := tx.ExecContext(ctx, `INSERT INTO gate_evidence_requirements (gate_id,ordinal,kind,claim) VALUES ($1,$2,$3,$4)`,
					gate.ID, index, requirement.Kind, requirement.Claim); err != nil {
					return mutationOutcome{}, nil, err
				}
			}
			if service.config.FaultInjection && request.HTTPRequest.Header.Get("X-Workplane-Fault") == "after-gate" {
				return mutationOutcome{}, nil, ErrInjectedCrash
			}
			eventID, err := appendReviewEvent(ctx, tx, service, request, actor, rid, &project, "gate.created", gate, now, "after-gate-event")
			if err != nil {
				return mutationOutcome{}, nil, err
			}
			return mutationOutcome{Status: http.StatusCreated, Body: gate, Version: project.Version, EventIDs: []string{eventID}}, nil, nil
		})
	return result, nil
}

func (service *Service) ListGates(ctx context.Context, request generated.Request) (generated.Response, error) {
	id := request.HTTPRequest.PathValue("id")
	projectID, found := service.deliverableProjectID(ctx, id)
	actor, authResponse, ok := service.authenticate(ctx, request, "deliverable.read", projectID)
	if !ok {
		return authResponse, nil
	}
	rid := authResponse.Headers.XRequestID
	if !found || len(strings.TrimSpace(string(request.Body))) > 0 {
		return problem(http.StatusNotFound, "not_found", "Resource not found", "The requested resource is not available.", rid), nil
	}
	if denied, allowed := service.authorizeProject(ctx, actor, projectID, actor.OrganizationID, "deliverable.read", rid); !allowed {
		return denied, nil
	}
	rows, err := service.db.QueryContext(ctx, selectGate+` WHERE deliverable_id=$1 AND organization_id=$2 ORDER BY created_at,id`, id, actor.OrganizationID)
	if err != nil {
		return serviceUnavailable(rid), nil
	}
	defer rows.Close()
	items := make([]Gate, 0)
	for rows.Next() {
		item, err := scanGate(rows)
		if err != nil || loadGateRequirements(ctx, service.db, &item) != nil {
			return serviceUnavailable(rid), nil
		}
		items = append(items, item)
	}
	if err := rows.Err(); err != nil {
		return serviceUnavailable(rid), nil
	}
	return generated.Response{Status: http.StatusOK, Headers: generated.ResponseHeaders{XRequestID: rid}, Body: items}, nil
}

func loadCurrentEvidence(ctx context.Context, tx *sql.Tx, projectID string, ids []string) ([]Evidence, error) {
	items := make([]Evidence, 0, len(ids))
	for _, id := range ids {
		item, err := scanEvidence(tx.QueryRowContext(ctx, selectEvidence+` WHERE id=$1 AND project_id=$2
			AND NOT EXISTS (SELECT 1 FROM evidence newer WHERE newer.supersedes_id=evidence.id)`, id, projectID))
		if err != nil {
			return nil, err
		}
		if err := loadEvidenceSupports(ctx, tx, &item); err != nil {
			return nil, err
		}
		items = append(items, item)
	}
	return items, nil
}

func evidenceSupports(item Evidence, targetType, targetID string) bool {
	for _, support := range item.Supports {
		if support.TargetType == targetType && support.TargetID == targetID {
			return true
		}
	}
	return false
}

func evidenceMeetsGate(items []Evidence, gate Gate) bool {
	for _, requirement := range gate.RequiredEvidence {
		matched := false
		for _, item := range items {
			if item.Kind == requirement.Kind && item.Claim == requirement.Claim &&
				(evidenceSupports(item, "gate", gate.ID) || evidenceSupports(item, "deliverable", gate.DeliverableID)) {
				matched = true
				break
			}
		}
		if !matched {
			return false
		}
	}
	return true
}

func actorEffectiveIDs(actor Actor) map[string]bool {
	result := map[string]bool{actor.ID: true}
	if actor.PrincipalID != nil {
		result[*actor.PrincipalID] = true
	}
	return result
}

func overlapsEffectiveIdentity(ids map[string]bool, actorID string, principalID *string) bool {
	if ids[actorID] {
		return true
	}
	return principalID != nil && ids[*principalID]
}

func scanSubmission(row interface{ Scan(...any) error }) (Submission, error) {
	var item Submission
	var evidenceIDs pq.StringArray
	var principal sql.NullString
	var created time.Time
	if err := row.Scan(&item.ID, &item.OrganizationID, &item.ProjectID, &item.DeliverableID, &item.Kind, &evidenceIDs,
		&item.Note, &item.SubmittedBy, &item.SubmitterKind, &principal, &created); err != nil {
		return Submission{}, err
	}
	item.EvidenceIDs = []string(evidenceIDs)
	if principal.Valid {
		item.PrincipalID = &principal.String
	}
	item.CreatedAt = created.UTC().Format(timeFormat)
	return item, nil
}

func currentSubmission(ctx context.Context, tx *sql.Tx, deliverableID string) (Submission, error) {
	return scanSubmission(tx.QueryRowContext(ctx, `SELECT submission.id,submission.organization_id,submission.project_id,
		submission.deliverable_id,submission.kind::text,submission.evidence_ids,submission.note,submission.submitted_by,
		submission.submitter_kind::text,submission.principal_id,submission.created_at
		FROM deliverable_submission_heads head JOIN deliverable_submissions submission ON submission.id=head.submission_id
		WHERE head.deliverable_id=$1 FOR UPDATE OF head`, deliverableID))
}

func scanVerdict(row interface{ Scan(...any) error }) (Verdict, error) {
	var item Verdict
	var evidenceIDs pq.StringArray
	var principal, supersedes sql.NullString
	var created time.Time
	if err := row.Scan(&item.ID, &item.OrganizationID, &item.ProjectID, &item.DeliverableID, &item.GateID, &item.Result,
		&evidenceIDs, &item.ReviewerID, &item.ReviewerKind, &principal, &supersedes, &created); err != nil {
		return Verdict{}, err
	}
	item.EvidenceIDs = []string(evidenceIDs)
	if principal.Valid {
		item.PrincipalID = &principal.String
	}
	if supersedes.Valid {
		item.SupersedesID = &supersedes.String
	}
	item.CreatedAt = created.UTC().Format(timeFormat)
	return item, nil
}

func scanFinding(row interface{ Scan(...any) error }) (Finding, error) {
	var item Finding
	var created, updated time.Time
	if err := row.Scan(&item.ID, &item.OrganizationID, &item.ProjectID, &item.DeliverableID, &item.GateID,
		&item.VerdictID, &item.Title, &item.Detail, &item.Blocking, &item.State, &item.Version,
		&item.CreatedBy, &created, &updated); err != nil {
		return Finding{}, err
	}
	item.CreatedAt, item.UpdatedAt = created.UTC().Format(timeFormat), updated.UTC().Format(timeFormat)
	return item, nil
}

func normalizeVerdictInput(input generated.VerdictInput) (generated.VerdictInput, bool) {
	if input.Result != "pass" && input.Result != "fail" {
		return generated.VerdictInput{}, false
	}
	var ok bool
	if input.EvidenceIDs, ok = normalizeUUIDList(input.EvidenceIDs, 1, 64); !ok || len(input.Findings) > 32 {
		return generated.VerdictInput{}, false
	}
	if (input.Result == "pass" && len(input.Findings) != 0) || (input.Result == "fail" && len(input.Findings) == 0) {
		return generated.VerdictInput{}, false
	}
	for index := range input.Findings {
		item := &input.Findings[index]
		item.Title, item.Detail = strings.TrimSpace(item.Title), strings.TrimSpace(item.Detail)
		if !validText(item.Title, 1, 200) || !validText(item.Detail, 1, 4000) || item.Blocking == nil {
			return generated.VerdictInput{}, false
		}
	}
	return input, true
}

func independentReviewer(actor Actor, deliverable Deliverable, submission Submission, evidence []Evidence) bool {
	effective := actorEffectiveIDs(actor)
	if effective[deliverable.CreatedBy] || overlapsEffectiveIdentity(effective, submission.SubmittedBy, submission.PrincipalID) {
		return false
	}
	for _, item := range evidence {
		if overlapsEffectiveIdentity(effective, item.ProducedBy, item.PrincipalID) {
			return false
		}
	}
	return true
}

func (service *Service) RecordVerdict(ctx context.Context, request generated.Request) (generated.Response, error) {
	gateID := request.HTTPRequest.PathValue("gate_id")
	projectID, found := service.projectIDForGate(ctx, gateID)
	if !found {
		projectID = "00000000-0000-4000-8000-000000000000"
	}
	actor, rid, expected, response, ok := service.prepareProjectMutation(ctx, request, []string{"review.verdict"}, projectID)
	if !ok {
		return response, nil
	}
	input, err := decodeStrict[generated.VerdictInput](request.Body)
	input, valid := normalizeVerdictInput(input)
	if err != nil || !valid {
		return problem(http.StatusBadRequest, "invalid_request", "Invalid verdict", "Pass verdicts contain evidence only; fail verdicts require actionable findings.", rid), nil
	}
	result := service.executeProjectMutation(ctx, request, actor, rid, projectID, "recordVerdict", expected, canonicalJSON(input),
		func(tx *sql.Tx, project Project, actor Actor, rid string, now time.Time) (mutationOutcome, *generated.Response, error) {
			gate, err := loadGate(ctx, tx, gateID, true)
			if err != nil || gate.ProjectID != project.ID {
				return mutationOutcome{}, rejected(problem(http.StatusNotFound, "not_found", "Resource not found", "The requested resource is not available.", rid)), nil
			}
			deliverable, err := scanDeliverable(tx.QueryRowContext(ctx, selectDeliverable+` WHERE id=$1 AND project_id=$2 FOR UPDATE`, gate.DeliverableID, project.ID))
			if err != nil {
				return mutationOutcome{}, nil, err
			}
			if deliverable.State != "submitted" || gate.State == "waived" || gate.State == "passed" ||
				(input.Result == "fail" && gate.State != "pending") || (input.Result == "pass" && gate.State != "pending" && gate.State != "failed") {
				return mutationOutcome{}, rejected(problem(http.StatusConflict, "invariant_violation", "Verdict not allowed", "The gate or deliverable state does not accept this verdict.", rid)), nil
			}
			evidence, err := loadCurrentEvidence(ctx, tx, project.ID, input.EvidenceIDs)
			if err != nil || !evidenceMeetsGate(evidence, gate) {
				return mutationOutcome{}, rejected(problem(http.StatusConflict, "invariant_violation", "Evidence contract incomplete", "The verdict requires current evidence satisfying every declared gate claim.", rid)), nil
			}
			submission, err := currentSubmission(ctx, tx, deliverable.ID)
			if err != nil {
				return mutationOutcome{}, rejected(problem(http.StatusConflict, "invariant_violation", "Submission missing", "A gate verdict requires the immutable current submission.", rid)), nil
			}
			if gate.IndependenceRequired && !independentReviewer(actor, deliverable, submission, evidence) {
				return mutationOutcome{}, rejected(problem(http.StatusForbidden, "forbidden", "Independent review required", "The effective principal produced the submitted deliverable or evidence.", rid)), nil
			}
			var prior sql.NullString
			if err := tx.QueryRowContext(ctx, `SELECT id FROM review_verdicts WHERE gate_id=$1 ORDER BY created_at DESC,id DESC LIMIT 1`, gate.ID).Scan(&prior); err != nil && !errors.Is(err, sql.ErrNoRows) {
				return mutationOutcome{}, nil, err
			}
			verdictID, err := newUUID()
			if err != nil {
				return mutationOutcome{}, nil, err
			}
			var supersedes *string
			if prior.Valid {
				supersedes = &prior.String
			}
			verdict := Verdict{ID: verdictID, OrganizationID: actor.OrganizationID, ProjectID: project.ID,
				DeliverableID: deliverable.ID, GateID: gate.ID, Result: input.Result, EvidenceIDs: input.EvidenceIDs,
				ReviewerID: actor.ID, ReviewerKind: actor.Kind, PrincipalID: actor.PrincipalID, SupersedesID: supersedes,
				CreatedAt: now.Format(timeFormat)}
			if _, err := tx.ExecContext(ctx, `INSERT INTO review_verdicts
				(id,organization_id,project_id,deliverable_id,gate_id,result,evidence_ids,reviewer_id,reviewer_kind,principal_id,supersedes_id,created_at)
				VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)`, verdict.ID, verdict.OrganizationID, verdict.ProjectID,
				verdict.DeliverableID, verdict.GateID, verdict.Result, pq.Array(verdict.EvidenceIDs), verdict.ReviewerID,
				verdict.ReviewerKind, verdict.PrincipalID, verdict.SupersedesID, now); err != nil {
				return mutationOutcome{}, nil, err
			}
			gate.State = map[string]string{"pass": "passed", "fail": "failed"}[input.Result]
			gate.Version++
			gate.UpdatedAt = now.Format(timeFormat)
			if _, err := tx.ExecContext(ctx, `UPDATE review_gates SET state=$1,version=$2,updated_at=$3 WHERE id=$4`, gate.State, gate.Version, now, gate.ID); err != nil {
				return mutationOutcome{}, nil, err
			}
			if service.config.FaultInjection && request.HTTPRequest.Header.Get("X-Workplane-Fault") == "after-verdict" {
				return mutationOutcome{}, nil, ErrInjectedCrash
			}
			eventIDs := make([]string, 0, 1+len(input.Findings))
			eventID, err := appendReviewEvent(ctx, tx, service, request, actor, rid, &project, "gate.verdict_recorded",
				GateVerdictEvent{Gate: gate, Verdict: verdict}, now, "after-verdict-event")
			if err != nil {
				return mutationOutcome{}, nil, err
			}
			eventIDs = append(eventIDs, eventID)
			findings := make([]Finding, 0, len(input.Findings))
			for _, findingInput := range input.Findings {
				findingID, err := newUUID()
				if err != nil {
					return mutationOutcome{}, nil, err
				}
				finding := Finding{ID: findingID, OrganizationID: actor.OrganizationID, ProjectID: project.ID,
					DeliverableID: deliverable.ID, GateID: gate.ID, VerdictID: verdict.ID, Title: findingInput.Title,
					Detail: findingInput.Detail, Blocking: *findingInput.Blocking, State: "open", Version: 1,
					CreatedBy: actor.ID, CreatedAt: now.Format(timeFormat), UpdatedAt: now.Format(timeFormat)}
				if _, err := tx.ExecContext(ctx, `INSERT INTO review_findings
					(id,organization_id,project_id,deliverable_id,gate_id,verdict_id,title,detail,blocking,state,version,created_by,created_at,updated_at)
					VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,'open',1,$10,$11,$11)`, finding.ID, finding.OrganizationID,
					finding.ProjectID, finding.DeliverableID, finding.GateID, finding.VerdictID, finding.Title, finding.Detail,
					finding.Blocking, finding.CreatedBy, now); err != nil {
					return mutationOutcome{}, nil, err
				}
				if service.config.FaultInjection && request.HTTPRequest.Header.Get("X-Workplane-Fault") == "after-finding" {
					return mutationOutcome{}, nil, ErrInjectedCrash
				}
				findingEvent, err := appendReviewEvent(ctx, tx, service, request, actor, rid, &project, "finding.created", finding, now, "after-finding-event")
				if err != nil {
					return mutationOutcome{}, nil, err
				}
				eventIDs = append(eventIDs, findingEvent)
				findings = append(findings, finding)
			}
			body := VerdictResult{Gate: gate, Verdict: verdict, Findings: findings}
			return mutationOutcome{Status: http.StatusCreated, Body: body, Version: project.Version, EventIDs: eventIDs}, nil, nil
		})
	return result, nil
}

func (service *Service) ListVerdicts(ctx context.Context, request generated.Request) (generated.Response, error) {
	gateID := request.HTTPRequest.PathValue("gate_id")
	projectID, found := service.projectIDForGate(ctx, gateID)
	actor, authResponse, ok := service.authenticate(ctx, request, "deliverable.read", projectID)
	if !ok {
		return authResponse, nil
	}
	rid := authResponse.Headers.XRequestID
	if !found || len(strings.TrimSpace(string(request.Body))) > 0 {
		return problem(http.StatusNotFound, "not_found", "Resource not found", "The requested resource is not available.", rid), nil
	}
	if denied, allowed := service.authorizeProject(ctx, actor, projectID, actor.OrganizationID, "deliverable.read", rid); !allowed {
		return denied, nil
	}
	rows, err := service.db.QueryContext(ctx, selectVerdict+` WHERE gate_id=$1 AND organization_id=$2 ORDER BY created_at,id`, gateID, actor.OrganizationID)
	if err != nil {
		return serviceUnavailable(rid), nil
	}
	defer rows.Close()
	items := make([]Verdict, 0)
	for rows.Next() {
		item, err := scanVerdict(rows)
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

func (service *Service) ListFindings(ctx context.Context, request generated.Request) (generated.Response, error) {
	id := request.HTTPRequest.PathValue("id")
	projectID, found := service.deliverableProjectID(ctx, id)
	actor, authResponse, ok := service.authenticate(ctx, request, "deliverable.read", projectID)
	if !ok {
		return authResponse, nil
	}
	rid := authResponse.Headers.XRequestID
	if !found || len(strings.TrimSpace(string(request.Body))) > 0 {
		return problem(http.StatusNotFound, "not_found", "Resource not found", "The requested resource is not available.", rid), nil
	}
	if denied, allowed := service.authorizeProject(ctx, actor, projectID, actor.OrganizationID, "deliverable.read", rid); !allowed {
		return denied, nil
	}
	rows, err := service.db.QueryContext(ctx, selectFinding+` WHERE deliverable_id=$1 AND organization_id=$2 ORDER BY created_at,id`, id, actor.OrganizationID)
	if err != nil {
		return serviceUnavailable(rid), nil
	}
	defer rows.Close()
	items := make([]Finding, 0)
	for rows.Next() {
		item, err := scanFinding(rows)
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

func normalizeFindingDisposition(input generated.FindingDispositionInput) (generated.FindingDispositionInput, bool) {
	var ok bool
	input.Rationale = strings.TrimSpace(input.Rationale)
	if input.EvidenceIDs, ok = normalizeUUIDList(input.EvidenceIDs, 1, 64); !ok || !validText(input.Rationale, 1, 4000) {
		return generated.FindingDispositionInput{}, false
	}
	return input, true
}

func (service *Service) disposeFinding(ctx context.Context, request generated.Request, action string) (generated.Response, error) {
	findingID := request.HTTPRequest.PathValue("finding_id")
	projectID, found := service.projectIDForFinding(ctx, findingID)
	if !found {
		projectID = "00000000-0000-4000-8000-000000000000"
	}
	actionPermission := "finding.resolve"
	if action == "withdraw" {
		actionPermission = "finding.withdraw"
	}
	actor, rid, expected, response, ok := service.prepareProjectMutation(ctx, request, []string{actionPermission}, projectID)
	if !ok {
		return response, nil
	}
	input, err := decodeStrict[generated.FindingDispositionInput](request.Body)
	input, valid := normalizeFindingDisposition(input)
	if err != nil || !valid {
		return problem(http.StatusBadRequest, "invalid_request", "Invalid finding disposition", "A disposition requires current attributable evidence and rationale.", rid), nil
	}
	result := service.executeProjectMutation(ctx, request, actor, rid, projectID, action+"Finding", expected, canonicalJSON(input),
		func(tx *sql.Tx, project Project, actor Actor, rid string, now time.Time) (mutationOutcome, *generated.Response, error) {
			finding, err := scanFinding(tx.QueryRowContext(ctx, selectFinding+` WHERE id=$1 AND project_id=$2 FOR UPDATE`, findingID, project.ID))
			if err != nil {
				return mutationOutcome{}, rejected(problem(http.StatusNotFound, "not_found", "Resource not found", "The requested resource is not available.", rid)), nil
			}
			if finding.State != "open" {
				return mutationOutcome{}, rejected(problem(http.StatusConflict, "invariant_violation", "Finding already closed", "Finding disposition history cannot be rewritten.", rid)), nil
			}
			evidence, err := loadCurrentEvidence(ctx, tx, project.ID, input.EvidenceIDs)
			if err != nil {
				return mutationOutcome{}, rejected(problem(http.StatusConflict, "invariant_violation", "Resolution evidence invalid", "Every finding disposition requires current project evidence.", rid)), nil
			}
			for _, item := range evidence {
				if !evidenceSupports(item, "finding", finding.ID) && !evidenceSupports(item, "deliverable", finding.DeliverableID) {
					return mutationOutcome{}, rejected(problem(http.StatusConflict, "invariant_violation", "Resolution evidence unlinked", "Disposition evidence must support the finding or its deliverable.", rid)), nil
				}
			}
			actionID, err := newUUID()
			if err != nil {
				return mutationOutcome{}, nil, err
			}
			finding.State, finding.Version, finding.UpdatedAt = map[string]string{"resolve": "resolved", "withdraw": "withdrawn"}[action], finding.Version+1, now.Format(timeFormat)
			if _, err := tx.ExecContext(ctx, `UPDATE review_findings SET state=$1,version=$2,updated_at=$3 WHERE id=$4`,
				finding.State, finding.Version, now, finding.ID); err != nil {
				return mutationOutcome{}, nil, err
			}
			disposition := FindingAction{ID: actionID, OrganizationID: actor.OrganizationID, ProjectID: project.ID,
				DeliverableID: finding.DeliverableID, FindingID: finding.ID, Action: action, EvidenceIDs: input.EvidenceIDs,
				Rationale: input.Rationale, ActorID: actor.ID, ActorKind: actor.Kind, PrincipalID: actor.PrincipalID, CreatedAt: now.Format(timeFormat)}
			if _, err := tx.ExecContext(ctx, `INSERT INTO finding_actions
				(id,organization_id,project_id,deliverable_id,finding_id,action,evidence_ids,rationale,actor_id,actor_kind,principal_id,created_at)
				VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)`, disposition.ID, disposition.OrganizationID,
				disposition.ProjectID, disposition.DeliverableID, disposition.FindingID, disposition.Action,
				pq.Array(disposition.EvidenceIDs), disposition.Rationale, disposition.ActorID, disposition.ActorKind,
				disposition.PrincipalID, now); err != nil {
				return mutationOutcome{}, nil, err
			}
			if service.config.FaultInjection && request.HTTPRequest.Header.Get("X-Workplane-Fault") == "after-finding-action" {
				return mutationOutcome{}, nil, ErrInjectedCrash
			}
			body := FindingActionResult{Finding: finding, Action: disposition}
			eventID, err := appendReviewEvent(ctx, tx, service, request, actor, rid, &project, "finding."+map[string]string{"resolve": "resolved", "withdraw": "withdrawn"}[action], body, now, "after-finding-action-event")
			if err != nil {
				return mutationOutcome{}, nil, err
			}
			return mutationOutcome{Status: http.StatusOK, Body: body, Version: project.Version, EventIDs: []string{eventID}}, nil, nil
		})
	return result, nil
}

func (service *Service) ResolveFinding(ctx context.Context, request generated.Request) (generated.Response, error) {
	return service.disposeFinding(ctx, request, "resolve")
}

func (service *Service) WithdrawFinding(ctx context.Context, request generated.Request) (generated.Response, error) {
	return service.disposeFinding(ctx, request, "withdraw")
}

func normalizeSubmissionInput(input generated.SubmissionInput) (generated.SubmissionInput, bool) {
	var ok bool
	input.Note = strings.TrimSpace(input.Note)
	if input.EvidenceIDs, ok = normalizeUUIDList(input.EvidenceIDs, 1, 64); !ok || !validText(input.Note, 1, 4000) {
		return generated.SubmissionInput{}, false
	}
	return input, true
}

func loadDeliverableGates(ctx context.Context, queryer databaseQueryer, deliverableID string) ([]Gate, error) {
	rows, err := queryer.QueryContext(ctx, selectGate+` WHERE deliverable_id=$1 ORDER BY created_at,id`, deliverableID)
	if err != nil {
		return nil, err
	}
	gates := make([]Gate, 0)
	for rows.Next() {
		gate, err := scanGate(rows)
		if err != nil {
			return nil, err
		}
		gates = append(gates, gate)
	}
	if err := rows.Err(); err != nil {
		_ = rows.Close()
		return nil, err
	}
	if err := rows.Close(); err != nil {
		return nil, err
	}
	for index := range gates {
		if err := loadGateRequirements(ctx, queryer, &gates[index]); err != nil {
			return nil, err
		}
	}
	return gates, nil
}

func evidenceMeetsDeliverableContract(items []Evidence, deliverableID string, gates []Gate) bool {
	if len(gates) == 0 {
		return false
	}
	for _, item := range items {
		relevant := evidenceSupports(item, "deliverable", deliverableID)
		for _, gate := range gates {
			relevant = relevant || evidenceSupports(item, "gate", gate.ID)
		}
		if !relevant {
			return false
		}
	}
	for _, gate := range gates {
		if !evidenceMeetsGate(items, gate) {
			return false
		}
	}
	return true
}

func insertSubmission(ctx context.Context, tx *sql.Tx, actor Actor, projectID, deliverableID, kind string,
	input generated.SubmissionInput, now time.Time) (Submission, error) {
	id, err := newUUID()
	if err != nil {
		return Submission{}, err
	}
	item := Submission{ID: id, OrganizationID: actor.OrganizationID, ProjectID: projectID, DeliverableID: deliverableID,
		Kind: kind, EvidenceIDs: input.EvidenceIDs, Note: input.Note, SubmittedBy: actor.ID, SubmitterKind: actor.Kind,
		PrincipalID: actor.PrincipalID, CreatedAt: now.Format(timeFormat)}
	if _, err := tx.ExecContext(ctx, `INSERT INTO deliverable_submissions
		(id,organization_id,project_id,deliverable_id,kind,evidence_ids,note,submitted_by,submitter_kind,principal_id,created_at)
		VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)`, item.ID, item.OrganizationID, item.ProjectID,
		item.DeliverableID, item.Kind, pq.Array(item.EvidenceIDs), item.Note, item.SubmittedBy, item.SubmitterKind,
		item.PrincipalID, now); err != nil {
		return Submission{}, err
	}
	if _, err := tx.ExecContext(ctx, `INSERT INTO deliverable_submission_heads (deliverable_id,submission_id,updated_at)
		VALUES ($1,$2,$3) ON CONFLICT (deliverable_id) DO UPDATE SET submission_id=EXCLUDED.submission_id,updated_at=EXCLUDED.updated_at`,
		deliverableID, item.ID, now); err != nil {
		return Submission{}, err
	}
	return item, nil
}

func (service *Service) submitDeliverable(ctx context.Context, request generated.Request, kind string) (generated.Response, error) {
	id := request.HTTPRequest.PathValue("id")
	projectID, found := service.deliverableProjectID(ctx, id)
	if !found {
		projectID = "00000000-0000-4000-8000-000000000000"
	}
	actor, rid, expected, response, ok := service.prepareProjectMutation(ctx, request, []string{"deliverable.submit", "review.request"}, projectID)
	if !ok {
		return response, nil
	}
	input, err := decodeStrict[generated.SubmissionInput](request.Body)
	input, valid := normalizeSubmissionInput(input)
	if err != nil || !valid {
		return problem(http.StatusBadRequest, "invalid_request", "Invalid submission", "Submission requires a note and a unique set of current evidence identifiers.", rid), nil
	}
	operation := "submitDeliverable"
	if kind == "resubmit" {
		operation = "resubmitDeliverable"
	}
	result := service.executeProjectMutation(ctx, request, actor, rid, projectID, operation, expected, canonicalJSON(input),
		func(tx *sql.Tx, project Project, actor Actor, rid string, now time.Time) (mutationOutcome, *generated.Response, error) {
			deliverable, err := scanDeliverable(tx.QueryRowContext(ctx, selectDeliverable+` WHERE id=$1 AND project_id=$2 FOR UPDATE`, id, project.ID))
			if err != nil {
				return mutationOutcome{}, rejected(problem(http.StatusNotFound, "not_found", "Resource not found", "The requested resource is not available.", rid)), nil
			}
			expectedState := "ready"
			if kind == "resubmit" {
				expectedState = "bounced"
			}
			if project.Mode != "exploitation" || !mutableProject(project) || deliverable.State != expectedState {
				return mutationOutcome{}, rejected(problem(http.StatusConflict, "invariant_violation", "Submission not allowed", "The deliverable is not in the required review state.", rid)), nil
			}
			evidence, err := loadCurrentEvidence(ctx, tx, project.ID, input.EvidenceIDs)
			if err != nil {
				return mutationOutcome{}, rejected(problem(http.StatusConflict, "invariant_violation", "Submission evidence invalid", "Submission evidence must be current and project-scoped.", rid)), nil
			}
			gates, err := loadDeliverableGates(ctx, tx, deliverable.ID)
			if err != nil || !evidenceMeetsDeliverableContract(evidence, deliverable.ID, gates) {
				return mutationOutcome{}, rejected(problem(http.StatusConflict, "invariant_violation", "Evidence contract incomplete", "Submission evidence must satisfy every attached gate contract.", rid)), nil
			}
			if kind == "resubmit" {
				var open int
				if err := tx.QueryRowContext(ctx, `SELECT count(*) FROM review_findings WHERE deliverable_id=$1 AND state='open'`, deliverable.ID).Scan(&open); err != nil {
					return mutationOutcome{}, nil, err
				}
				var hasResolution bool
				if err := tx.QueryRowContext(ctx, `SELECT EXISTS (SELECT 1 FROM finding_actions
					WHERE deliverable_id=$1 AND action='resolve' AND evidence_ids && $2::uuid[])`, deliverable.ID, pq.Array(input.EvidenceIDs)).Scan(&hasResolution); err != nil {
					return mutationOutcome{}, nil, err
				}
				if open != 0 || !hasResolution {
					return mutationOutcome{}, rejected(problem(http.StatusConflict, "invariant_violation", "Resolution evidence incomplete", "Resubmission requires every finding closed and linked resolution evidence.", rid)), nil
				}
			}
			submission, err := insertSubmission(ctx, tx, actor, project.ID, deliverable.ID, kind, input, now)
			if err != nil {
				return mutationOutcome{}, nil, err
			}
			deliverable.State, deliverable.Version, deliverable.UpdatedAt = "submitted", deliverable.Version+1, now.Format(timeFormat)
			if _, err := tx.ExecContext(ctx, `UPDATE deliverables SET state='submitted',version=$1,updated_at=$2 WHERE id=$3`, deliverable.Version, now, deliverable.ID); err != nil {
				return mutationOutcome{}, nil, err
			}
			if service.config.FaultInjection && request.HTTPRequest.Header.Get("X-Workplane-Fault") == "after-submission" {
				return mutationOutcome{}, nil, ErrInjectedCrash
			}
			body := SubmissionResult{Deliverable: deliverable, Submission: submission}
			eventType := "deliverable.submitted"
			if kind == "resubmit" {
				eventType = "deliverable.resubmitted"
			}
			eventID, err := appendReviewEvent(ctx, tx, service, request, actor, rid, &project, eventType, body, now, "after-submission-event")
			if err != nil {
				return mutationOutcome{}, nil, err
			}
			return mutationOutcome{Status: http.StatusOK, Body: body, Version: project.Version, EventIDs: []string{eventID}}, nil, nil
		})
	return result, nil
}

func (service *Service) SubmitDeliverable(ctx context.Context, request generated.Request) (generated.Response, error) {
	return service.submitDeliverable(ctx, request, "submit")
}

func (service *Service) ResubmitDeliverable(ctx context.Context, request generated.Request) (generated.Response, error) {
	return service.submitDeliverable(ctx, request, "resubmit")
}

func (service *Service) BounceDeliverable(ctx context.Context, request generated.Request) (generated.Response, error) {
	id := request.HTTPRequest.PathValue("id")
	projectID, found := service.deliverableProjectID(ctx, id)
	if !found {
		projectID = "00000000-0000-4000-8000-000000000000"
	}
	actor, rid, expected, response, ok := service.prepareProjectMutation(ctx, request, []string{"review.verdict"}, projectID)
	if !ok {
		return response, nil
	}
	input, err := decodeStrict[generated.BounceInput](request.Body)
	input.FindingID, input.Rationale = strings.ToLower(strings.TrimSpace(input.FindingID)), strings.TrimSpace(input.Rationale)
	if err != nil || !uuidPattern.MatchString(input.FindingID) || !validText(input.Rationale, 1, 4000) {
		return problem(http.StatusBadRequest, "invalid_request", "Invalid bounce", "Bounce requires one current actionable finding and rationale.", rid), nil
	}
	result := service.executeProjectMutation(ctx, request, actor, rid, projectID, "bounceDeliverable", expected, canonicalJSON(input),
		func(tx *sql.Tx, project Project, actor Actor, rid string, now time.Time) (mutationOutcome, *generated.Response, error) {
			deliverable, err := scanDeliverable(tx.QueryRowContext(ctx, selectDeliverable+` WHERE id=$1 AND project_id=$2 FOR UPDATE`, id, project.ID))
			if err != nil {
				return mutationOutcome{}, rejected(problem(http.StatusNotFound, "not_found", "Resource not found", "The requested resource is not available.", rid)), nil
			}
			var actionable bool
			if err := tx.QueryRowContext(ctx, `SELECT EXISTS (SELECT 1 FROM review_findings WHERE id=$1 AND deliverable_id=$2 AND state='open'
				AND length(btrim(detail))>0)`, input.FindingID, deliverable.ID).Scan(&actionable); err != nil {
				return mutationOutcome{}, nil, err
			}
			if deliverable.State != "submitted" || !actionable {
				return mutationOutcome{}, rejected(problem(http.StatusConflict, "invariant_violation", "Bounce not allowed", "A submitted deliverable and open actionable finding are required.", rid)), nil
			}
			deliverable.State, deliverable.Version, deliverable.UpdatedAt = "bounced", deliverable.Version+1, now.Format(timeFormat)
			if _, err := tx.ExecContext(ctx, `UPDATE deliverables SET state='bounced',version=$1,updated_at=$2 WHERE id=$3`, deliverable.Version, now, deliverable.ID); err != nil {
				return mutationOutcome{}, nil, err
			}
			if service.config.FaultInjection && request.HTTPRequest.Header.Get("X-Workplane-Fault") == "after-deliverable-review" {
				return mutationOutcome{}, nil, ErrInjectedCrash
			}
			body := DeliverableReviewEvent{Deliverable: deliverable, FindingID: &input.FindingID, Rationale: input.Rationale}
			eventID, err := appendReviewEvent(ctx, tx, service, request, actor, rid, &project, "deliverable.bounced", body, now, "after-deliverable-review-event")
			if err != nil {
				return mutationOutcome{}, nil, err
			}
			return mutationOutcome{Status: http.StatusOK, Body: deliverable, Version: project.Version, EventIDs: []string{eventID}}, nil, nil
		})
	return result, nil
}

func (service *Service) ApproveDeliverable(ctx context.Context, request generated.Request) (generated.Response, error) {
	id := request.HTTPRequest.PathValue("id")
	projectID, found := service.deliverableProjectID(ctx, id)
	if !found {
		projectID = "00000000-0000-4000-8000-000000000000"
	}
	actor, rid, expected, response, ok := service.prepareProjectMutation(ctx, request, []string{"review.verdict"}, projectID)
	if !ok {
		return response, nil
	}
	input, err := decodeStrict[generated.ReviewNoteInput](request.Body)
	input.Note = strings.TrimSpace(input.Note)
	if err != nil || !validText(input.Note, 1, 4000) {
		return problem(http.StatusBadRequest, "invalid_request", "Invalid approval", "Approval requires an attributable review note.", rid), nil
	}
	result := service.executeProjectMutation(ctx, request, actor, rid, projectID, "approveDeliverable", expected, canonicalJSON(input),
		func(tx *sql.Tx, project Project, actor Actor, rid string, now time.Time) (mutationOutcome, *generated.Response, error) {
			deliverable, err := scanDeliverable(tx.QueryRowContext(ctx, selectDeliverable+` WHERE id=$1 AND project_id=$2 FOR UPDATE`, id, project.ID))
			if err != nil {
				return mutationOutcome{}, rejected(problem(http.StatusNotFound, "not_found", "Resource not found", "The requested resource is not available.", rid)), nil
			}
			var gateCount, incompleteHard, openFindings int
			if err := tx.QueryRowContext(ctx, `SELECT count(*),count(*) FILTER (WHERE hard AND state<>'passed') FROM review_gates WHERE deliverable_id=$1`, id).Scan(&gateCount, &incompleteHard); err != nil {
				return mutationOutcome{}, nil, err
			}
			if err := tx.QueryRowContext(ctx, `SELECT count(*) FROM review_findings WHERE deliverable_id=$1 AND state='open'`, id).Scan(&openFindings); err != nil {
				return mutationOutcome{}, nil, err
			}
			if deliverable.State != "submitted" || gateCount == 0 || incompleteHard != 0 || openFindings != 0 {
				return mutationOutcome{}, rejected(problem(http.StatusConflict, "invariant_violation", "Approval contract incomplete", "Approval requires every hard gate passed and every finding resolved or withdrawn.", rid)), nil
			}
			deliverable.State, deliverable.Version, deliverable.UpdatedAt = "accepted", deliverable.Version+1, now.Format(timeFormat)
			if _, err := tx.ExecContext(ctx, `UPDATE deliverables SET state='accepted',version=$1,updated_at=$2 WHERE id=$3`, deliverable.Version, now, deliverable.ID); err != nil {
				return mutationOutcome{}, nil, err
			}
			if service.config.FaultInjection && request.HTTPRequest.Header.Get("X-Workplane-Fault") == "after-deliverable-review" {
				return mutationOutcome{}, nil, ErrInjectedCrash
			}
			body := DeliverableReviewEvent{Deliverable: deliverable, Rationale: input.Note}
			eventID, err := appendReviewEvent(ctx, tx, service, request, actor, rid, &project, "deliverable.accepted", body, now, "after-deliverable-review-event")
			if err != nil {
				return mutationOutcome{}, nil, err
			}
			return mutationOutcome{Status: http.StatusOK, Body: deliverable, Version: project.Version, EventIDs: []string{eventID}}, nil, nil
		})
	return result, nil
}

func normalizeWaiverInput(input generated.WaiverInput) (generated.WaiverInput, bool) {
	input.Question, input.Choice, input.Rationale, input.ResidualRisk = strings.TrimSpace(input.Question), strings.TrimSpace(input.Choice),
		strings.TrimSpace(input.Rationale), strings.TrimSpace(input.ResidualRisk)
	if !validText(input.Question, 1, 2000) || !validText(input.Choice, 1, 2000) || !validText(input.Rationale, 1, 4000) ||
		!validText(input.ResidualRisk, 1, 4000) {
		return generated.WaiverInput{}, false
	}
	var ok bool
	if input.Alternatives, ok = normalizeStrings(input.Alternatives, 0, 32, 0, 1000); !ok {
		return generated.WaiverInput{}, false
	}
	if input.Consequences, ok = normalizeStrings(input.Consequences, 0, 32, 0, 2000); !ok {
		return generated.WaiverInput{}, false
	}
	if input.EvidenceIDs, ok = normalizeUUIDList(input.EvidenceIDs, 1, 64); !ok {
		return generated.WaiverInput{}, false
	}
	return input, true
}

func insertWaiverDecision(ctx context.Context, tx *sql.Tx, actor Actor, projectID string, input generated.WaiverInput, now time.Time) (Decision, error) {
	id, err := newUUID()
	if err != nil {
		return Decision{}, err
	}
	consequences := append([]string{}, input.Consequences...)
	consequences = append(consequences, "residual-risk: "+input.ResidualRisk)
	decision := Decision{ID: id, ProjectID: projectID, ActorID: actor.ID, ActorKind: actor.Kind, PrincipalID: actor.PrincipalID,
		RecordedAt: now.Format(timeFormat), Kind: "waiver", Question: input.Question, Choice: input.Choice,
		Alternatives: input.Alternatives, Rationale: input.Rationale, Evidence: input.EvidenceIDs, Consequences: consequences}
	if _, err := tx.ExecContext(ctx, `INSERT INTO decisions
		(id,organization_id,project_id,kind,question,choice,alternatives,rationale,evidence,consequences,actor_id,recorded_at)
		VALUES ($1,$2,$3,'waiver',$4,$5,$6,$7,$8,$9,$10,$11)`, decision.ID, actor.OrganizationID, projectID,
		decision.Question, decision.Choice, canonicalJSON(decision.Alternatives), decision.Rationale, canonicalJSON(decision.Evidence),
		canonicalJSON(decision.Consequences), actor.ID, now); err != nil {
		return Decision{}, err
	}
	return decision, nil
}

func waiverPreconditions(ctx context.Context, tx *sql.Tx, deliverableID string) (bool, error) {
	var incompleteHard, blocking int
	if err := tx.QueryRowContext(ctx, `SELECT count(*) FROM review_gates WHERE deliverable_id=$1 AND hard AND state<>'passed'`, deliverableID).Scan(&incompleteHard); err != nil {
		return false, err
	}
	if err := tx.QueryRowContext(ctx, `SELECT count(*) FROM review_findings WHERE deliverable_id=$1 AND blocking AND state='open'`, deliverableID).Scan(&blocking); err != nil {
		return false, err
	}
	return incompleteHard == 0 && blocking == 0, nil
}

func (service *Service) WaiveDeliverable(ctx context.Context, request generated.Request) (generated.Response, error) {
	id := request.HTTPRequest.PathValue("id")
	projectID, found := service.deliverableProjectID(ctx, id)
	if !found {
		projectID = "00000000-0000-4000-8000-000000000000"
	}
	actor, rid, expected, response, ok := service.prepareProjectMutation(ctx, request, []string{"deliverable.waive", "decision.record"}, projectID)
	if !ok {
		return response, nil
	}
	if actor.Kind != "human" {
		return problem(http.StatusForbidden, "forbidden", "Human judgment required", "Deliverable waiver is policy-designated human authority.", rid), nil
	}
	input, err := decodeStrict[generated.WaiverInput](request.Body)
	input, valid := normalizeWaiverInput(input)
	if err != nil || !valid {
		return problem(http.StatusBadRequest, "invalid_request", "Invalid waiver", "Waiver requires a complete universal decision, evidence, rationale, and residual risk.", rid), nil
	}
	result := service.executeProjectMutation(ctx, request, actor, rid, projectID, "waiveDeliverable", expected, canonicalJSON(input),
		func(tx *sql.Tx, project Project, actor Actor, rid string, now time.Time) (mutationOutcome, *generated.Response, error) {
			deliverable, err := scanDeliverable(tx.QueryRowContext(ctx, selectDeliverable+` WHERE id=$1 AND project_id=$2 FOR UPDATE`, id, project.ID))
			if err != nil {
				return mutationOutcome{}, rejected(problem(http.StatusNotFound, "not_found", "Resource not found", "The requested resource is not available.", rid)), nil
			}
			preconditions, err := waiverPreconditions(ctx, tx, deliverable.ID)
			if err != nil {
				return mutationOutcome{}, nil, err
			}
			evidence, err := loadCurrentEvidence(ctx, tx, project.ID, input.EvidenceIDs)
			if err != nil {
				preconditions = false
			}
			gates, gateErr := loadDeliverableGates(ctx, tx, deliverable.ID)
			if gateErr != nil || !evidenceMeetsDeliverableContract(evidence, deliverable.ID, gates) {
				preconditions = false
			}
			if !map[string]bool{"ready": true, "submitted": true, "bounced": true}[deliverable.State] || !preconditions {
				return mutationOutcome{}, rejected(problem(http.StatusConflict, "invariant_violation", "Waiver contract incomplete", "Every hard gate must pass, no blocking finding may remain, and waiver evidence must support the deliverable.", rid)), nil
			}
			decision, err := insertWaiverDecision(ctx, tx, actor, project.ID, input, now)
			if err != nil {
				return mutationOutcome{}, nil, err
			}
			if service.config.FaultInjection && request.HTTPRequest.Header.Get("X-Workplane-Fault") == "after-decision" {
				return mutationOutcome{}, nil, ErrInjectedCrash
			}
			eventIDs := make([]string, 0, 2)
			decisionEvent, err := appendReviewEvent(ctx, tx, service, request, actor, rid, &project, "decision.recorded", decision, now, "after-waiver-decision-event")
			if err != nil {
				return mutationOutcome{}, nil, err
			}
			eventIDs = append(eventIDs, decisionEvent)
			deliverable.State, deliverable.Version, deliverable.WaiverDecisionID, deliverable.UpdatedAt = "waived", deliverable.Version+1, &decision.ID, now.Format(timeFormat)
			if _, err := tx.ExecContext(ctx, `UPDATE deliverables SET state='waived',version=$1,waiver_decision_id=$2,updated_at=$3 WHERE id=$4`,
				deliverable.Version, decision.ID, now, deliverable.ID); err != nil {
				return mutationOutcome{}, nil, err
			}
			body := DeliverableWaiverResult{Deliverable: deliverable, Decision: decision}
			waiverEvent, err := appendReviewEvent(ctx, tx, service, request, actor, rid, &project, "deliverable.waived", body, now, "after-deliverable-waiver-event")
			if err != nil {
				return mutationOutcome{}, nil, err
			}
			eventIDs = append(eventIDs, waiverEvent)
			return mutationOutcome{Status: http.StatusOK, Body: body, Version: project.Version, EventIDs: eventIDs}, nil, nil
		})
	return result, nil
}

func (service *Service) WaiveGate(ctx context.Context, request generated.Request) (generated.Response, error) {
	gateID := request.HTTPRequest.PathValue("gate_id")
	projectID, found := service.projectIDForGate(ctx, gateID)
	if !found {
		projectID = "00000000-0000-4000-8000-000000000000"
	}
	actor, rid, expected, response, ok := service.prepareProjectMutation(ctx, request, []string{"gate.soft_waive", "decision.record"}, projectID)
	if !ok {
		return response, nil
	}
	if actor.Kind != "human" {
		return problem(http.StatusForbidden, "forbidden", "Human judgment required", "Soft-gate waiver is policy-designated human authority.", rid), nil
	}
	input, err := decodeStrict[generated.WaiverInput](request.Body)
	input, valid := normalizeWaiverInput(input)
	if err != nil || !valid {
		return problem(http.StatusBadRequest, "invalid_request", "Invalid waiver", "Waiver requires a complete universal decision, evidence, rationale, and residual risk.", rid), nil
	}
	result := service.executeProjectMutation(ctx, request, actor, rid, projectID, "waiveGate", expected, canonicalJSON(input),
		func(tx *sql.Tx, project Project, actor Actor, rid string, now time.Time) (mutationOutcome, *generated.Response, error) {
			gate, err := loadGate(ctx, tx, gateID, true)
			if err != nil || gate.ProjectID != project.ID {
				return mutationOutcome{}, rejected(problem(http.StatusNotFound, "not_found", "Resource not found", "The requested resource is not available.", rid)), nil
			}
			preconditions, err := waiverPreconditions(ctx, tx, gate.DeliverableID)
			if err != nil {
				return mutationOutcome{}, nil, err
			}
			evidence, err := loadCurrentEvidence(ctx, tx, project.ID, input.EvidenceIDs)
			if err != nil {
				preconditions = false
			}
			if !evidenceMeetsGate(evidence, gate) {
				preconditions = false
			}
			if gate.Hard || !map[string]bool{"pending": true, "failed": true}[gate.State] || !preconditions {
				return mutationOutcome{}, rejected(problem(http.StatusConflict, "invariant_violation", "Gate waiver forbidden", "Hard gates are never waivable; soft waivers require passed hard gates and no blocking finding.", rid)), nil
			}
			decision, err := insertWaiverDecision(ctx, tx, actor, project.ID, input, now)
			if err != nil {
				return mutationOutcome{}, nil, err
			}
			if service.config.FaultInjection && request.HTTPRequest.Header.Get("X-Workplane-Fault") == "after-decision" {
				return mutationOutcome{}, nil, ErrInjectedCrash
			}
			eventIDs := make([]string, 0, 2)
			decisionEvent, err := appendReviewEvent(ctx, tx, service, request, actor, rid, &project, "decision.recorded", decision, now, "after-waiver-decision-event")
			if err != nil {
				return mutationOutcome{}, nil, err
			}
			eventIDs = append(eventIDs, decisionEvent)
			gate.State, gate.Version, gate.WaiverDecisionID, gate.UpdatedAt = "waived", gate.Version+1, &decision.ID, now.Format(timeFormat)
			if _, err := tx.ExecContext(ctx, `UPDATE review_gates SET state='waived',version=$1,waiver_decision_id=$2,updated_at=$3 WHERE id=$4`,
				gate.Version, decision.ID, now, gate.ID); err != nil {
				return mutationOutcome{}, nil, err
			}
			body := GateWaiverResult{Gate: gate, Decision: decision}
			waiverEvent, err := appendReviewEvent(ctx, tx, service, request, actor, rid, &project, "gate.waived", body, now, "after-gate-waiver-event")
			if err != nil {
				return mutationOutcome{}, nil, err
			}
			eventIDs = append(eventIDs, waiverEvent)
			return mutationOutcome{Status: http.StatusOK, Body: body, Version: project.Version, EventIDs: eventIDs}, nil, nil
		})
	return result, nil
}

func reviewProjectionKey(kind, id string) string { return kind + ":" + id }

func sortReviewSnapshot(snapshot *projectionSnapshot) {
	sort.Slice(snapshot.Evidence, func(i, j int) bool { return snapshot.Evidence[i].ID < snapshot.Evidence[j].ID })
	sort.Slice(snapshot.Gates, func(i, j int) bool { return snapshot.Gates[i].ID < snapshot.Gates[j].ID })
	sort.Slice(snapshot.Verdicts, func(i, j int) bool { return snapshot.Verdicts[i].ID < snapshot.Verdicts[j].ID })
	sort.Slice(snapshot.Findings, func(i, j int) bool { return snapshot.Findings[i].ID < snapshot.Findings[j].ID })
	sort.Slice(snapshot.FindingActions, func(i, j int) bool { return snapshot.FindingActions[i].ID < snapshot.FindingActions[j].ID })
	sort.Slice(snapshot.Submissions, func(i, j int) bool { return snapshot.Submissions[i].ID < snapshot.Submissions[j].ID })
	sort.Slice(snapshot.SubmissionHeads, func(i, j int) bool {
		return snapshot.SubmissionHeads[i].DeliverableID < snapshot.SubmissionHeads[j].DeliverableID
	})
}
