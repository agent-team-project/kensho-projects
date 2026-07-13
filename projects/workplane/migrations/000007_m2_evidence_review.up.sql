BEGIN;

CREATE TYPE evidence_kind AS ENUM
  ('report', 'test-run', 'link', 'image', 'observation', 'measurement');
CREATE TYPE evidence_target_kind AS ENUM ('deliverable', 'gate', 'finding', 'decision');
CREATE TYPE review_gate_kind AS ENUM ('automated', 'review', 'approval');
CREATE TYPE review_gate_state AS ENUM ('pending', 'passed', 'failed', 'waived');
CREATE TYPE review_verdict_result AS ENUM ('pass', 'fail');
CREATE TYPE review_finding_state AS ENUM ('open', 'resolved', 'withdrawn');
CREATE TYPE finding_action_kind AS ENUM ('resolve', 'withdraw');
CREATE TYPE submission_kind AS ENUM ('submit', 'resubmit');

CREATE TABLE evidence (
  id uuid PRIMARY KEY,
  organization_id uuid NOT NULL REFERENCES organizations(id),
  project_id uuid NOT NULL REFERENCES projects(id),
  kind evidence_kind NOT NULL,
  title text NOT NULL CHECK (length(title) BETWEEN 1 AND 200),
  claim text NOT NULL CHECK (length(claim) BETWEEN 1 AND 2000),
  source text NOT NULL CHECK (length(source) BETWEEN 1 AND 2000),
  content text CHECK (content IS NULL OR length(content) BETWEEN 1 AND 16000),
  uri text CHECK (uri IS NULL OR length(uri) BETWEEN 1 AND 2000),
  metadata jsonb NOT NULL CHECK (jsonb_typeof(metadata) = 'object'),
  integrity_material jsonb NOT NULL CHECK (jsonb_typeof(integrity_material) = 'object'),
  integrity_digest bytea NOT NULL CHECK (octet_length(integrity_digest) = 32),
  produced_by uuid NOT NULL REFERENCES principals(id),
  producer_kind principal_kind NOT NULL,
  principal_id uuid REFERENCES principals(id),
  produced_at timestamptz NOT NULL,
  supersedes_id uuid,
  CHECK (content IS NOT NULL OR uri IS NOT NULL),
  CHECK ((producer_kind = 'human' AND principal_id IS NULL)
    OR (producer_kind = 'agent' AND principal_id IS NOT NULL)),
  CHECK (supersedes_id IS NULL OR supersedes_id <> id),
  UNIQUE (organization_id, project_id, id),
  UNIQUE (project_id, id),
  UNIQUE (supersedes_id),
  FOREIGN KEY (organization_id, project_id) REFERENCES projects(organization_id, id),
  FOREIGN KEY (project_id, supersedes_id) REFERENCES evidence(project_id, id)
);

CREATE TABLE evidence_links (
  project_id uuid NOT NULL,
  evidence_id uuid NOT NULL,
  target_kind evidence_target_kind NOT NULL,
  target_id uuid NOT NULL,
  PRIMARY KEY (evidence_id, target_kind, target_id),
  FOREIGN KEY (project_id, evidence_id) REFERENCES evidence(project_id, id)
);

CREATE TABLE review_gates (
  id uuid PRIMARY KEY,
  organization_id uuid NOT NULL REFERENCES organizations(id),
  project_id uuid NOT NULL REFERENCES projects(id),
  deliverable_id uuid NOT NULL REFERENCES deliverables(id),
  name text NOT NULL CHECK (length(name) BETWEEN 1 AND 200),
  kind review_gate_kind NOT NULL,
  hard boolean NOT NULL,
  independence_required boolean NOT NULL,
  state review_gate_state NOT NULL DEFAULT 'pending',
  version bigint NOT NULL DEFAULT 1 CHECK (version >= 1),
  waiver_decision_id uuid REFERENCES decisions(id),
  created_by uuid NOT NULL REFERENCES principals(id),
  created_at timestamptz NOT NULL,
  updated_at timestamptz NOT NULL,
  CHECK (NOT hard OR state <> 'waived'),
  CHECK (state = 'waived' OR waiver_decision_id IS NULL),
  UNIQUE (organization_id, project_id, id),
  UNIQUE (project_id, id),
  UNIQUE (deliverable_id, id),
  FOREIGN KEY (organization_id, project_id) REFERENCES projects(organization_id, id),
  FOREIGN KEY (project_id, deliverable_id) REFERENCES deliverables(project_id, id)
);

CREATE TABLE gate_evidence_requirements (
  gate_id uuid NOT NULL REFERENCES review_gates(id),
  ordinal smallint NOT NULL CHECK (ordinal BETWEEN 0 AND 31),
  kind evidence_kind NOT NULL,
  claim text NOT NULL CHECK (length(claim) BETWEEN 1 AND 2000),
  PRIMARY KEY (gate_id, ordinal),
  UNIQUE (gate_id, kind, claim)
);

CREATE TABLE review_verdicts (
  id uuid PRIMARY KEY,
  organization_id uuid NOT NULL REFERENCES organizations(id),
  project_id uuid NOT NULL REFERENCES projects(id),
  deliverable_id uuid NOT NULL REFERENCES deliverables(id),
  gate_id uuid NOT NULL REFERENCES review_gates(id),
  result review_verdict_result NOT NULL,
  evidence_ids uuid[] NOT NULL CHECK (cardinality(evidence_ids) BETWEEN 1 AND 64),
  reviewer_id uuid NOT NULL REFERENCES principals(id),
  reviewer_kind principal_kind NOT NULL,
  principal_id uuid REFERENCES principals(id),
  supersedes_id uuid,
  created_at timestamptz NOT NULL,
  CHECK ((reviewer_kind = 'human' AND principal_id IS NULL)
    OR (reviewer_kind = 'agent' AND principal_id IS NOT NULL)),
  CHECK (supersedes_id IS NULL OR supersedes_id <> id),
  UNIQUE (project_id, id),
  UNIQUE (gate_id, id),
  UNIQUE (supersedes_id),
  FOREIGN KEY (organization_id, project_id) REFERENCES projects(organization_id, id),
  FOREIGN KEY (deliverable_id, gate_id) REFERENCES review_gates(deliverable_id, id),
  FOREIGN KEY (project_id, supersedes_id) REFERENCES review_verdicts(project_id, id)
);

CREATE TABLE review_findings (
  id uuid PRIMARY KEY,
  organization_id uuid NOT NULL REFERENCES organizations(id),
  project_id uuid NOT NULL REFERENCES projects(id),
  deliverable_id uuid NOT NULL REFERENCES deliverables(id),
  gate_id uuid NOT NULL REFERENCES review_gates(id),
  verdict_id uuid NOT NULL REFERENCES review_verdicts(id),
  title text NOT NULL CHECK (length(title) BETWEEN 1 AND 200),
  detail text NOT NULL CHECK (length(detail) BETWEEN 1 AND 4000),
  blocking boolean NOT NULL,
  state review_finding_state NOT NULL DEFAULT 'open',
  version bigint NOT NULL DEFAULT 1 CHECK (version >= 1),
  created_by uuid NOT NULL REFERENCES principals(id),
  created_at timestamptz NOT NULL,
  updated_at timestamptz NOT NULL,
  UNIQUE (organization_id, project_id, id),
  UNIQUE (project_id, id),
  UNIQUE (deliverable_id, id),
  FOREIGN KEY (organization_id, project_id) REFERENCES projects(organization_id, id),
  FOREIGN KEY (deliverable_id, gate_id) REFERENCES review_gates(deliverable_id, id),
  FOREIGN KEY (gate_id, verdict_id) REFERENCES review_verdicts(gate_id, id)
);

CREATE TABLE finding_actions (
  id uuid PRIMARY KEY,
  organization_id uuid NOT NULL REFERENCES organizations(id),
  project_id uuid NOT NULL REFERENCES projects(id),
  deliverable_id uuid NOT NULL REFERENCES deliverables(id),
  finding_id uuid NOT NULL REFERENCES review_findings(id),
  action finding_action_kind NOT NULL,
  evidence_ids uuid[] NOT NULL CHECK (cardinality(evidence_ids) BETWEEN 1 AND 64),
  rationale text NOT NULL CHECK (length(rationale) BETWEEN 1 AND 4000),
  actor_id uuid NOT NULL REFERENCES principals(id),
  actor_kind principal_kind NOT NULL,
  principal_id uuid REFERENCES principals(id),
  created_at timestamptz NOT NULL,
  CHECK ((actor_kind = 'human' AND principal_id IS NULL)
    OR (actor_kind = 'agent' AND principal_id IS NOT NULL)),
  FOREIGN KEY (organization_id, project_id) REFERENCES projects(organization_id, id),
  FOREIGN KEY (deliverable_id, finding_id) REFERENCES review_findings(deliverable_id, id)
);

CREATE TABLE deliverable_submissions (
  id uuid PRIMARY KEY,
  organization_id uuid NOT NULL REFERENCES organizations(id),
  project_id uuid NOT NULL REFERENCES projects(id),
  deliverable_id uuid NOT NULL REFERENCES deliverables(id),
  kind submission_kind NOT NULL,
  evidence_ids uuid[] NOT NULL CHECK (cardinality(evidence_ids) BETWEEN 1 AND 64),
  note text NOT NULL CHECK (length(note) BETWEEN 1 AND 4000),
  submitted_by uuid NOT NULL REFERENCES principals(id),
  submitter_kind principal_kind NOT NULL,
  principal_id uuid REFERENCES principals(id),
  created_at timestamptz NOT NULL,
  CHECK ((submitter_kind = 'human' AND principal_id IS NULL)
    OR (submitter_kind = 'agent' AND principal_id IS NOT NULL)),
  UNIQUE (deliverable_id, id),
  FOREIGN KEY (organization_id, project_id) REFERENCES projects(organization_id, id),
  FOREIGN KEY (project_id, deliverable_id) REFERENCES deliverables(project_id, id)
);

CREATE TABLE deliverable_submission_heads (
  deliverable_id uuid PRIMARY KEY REFERENCES deliverables(id),
  submission_id uuid NOT NULL UNIQUE,
  updated_at timestamptz NOT NULL,
  FOREIGN KEY (deliverable_id, submission_id)
    REFERENCES deliverable_submissions(deliverable_id, id)
);

ALTER TABLE deliverables ADD COLUMN waiver_decision_id uuid REFERENCES decisions(id);

CREATE FUNCTION verify_evidence_integrity() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.integrity_digest IS DISTINCT FROM
      digest(convert_to(NEW.integrity_material::text, 'UTF8'), 'sha256') THEN
    RAISE EXCEPTION 'evidence % integrity digest does not match canonical material', NEW.id
      USING ERRCODE = '23514';
  END IF;
  RETURN NEW;
END;
$$;

CREATE TRIGGER evidence_integrity
  BEFORE INSERT ON evidence
  FOR EACH ROW EXECUTE FUNCTION verify_evidence_integrity();
CREATE TRIGGER evidence_append_only
  BEFORE UPDATE OR DELETE ON evidence
  FOR EACH ROW EXECUTE FUNCTION reject_immutable_change();
CREATE TRIGGER evidence_links_append_only
  BEFORE UPDATE OR DELETE ON evidence_links
  FOR EACH ROW EXECUTE FUNCTION reject_immutable_change();
CREATE TRIGGER gate_requirements_append_only
  BEFORE UPDATE OR DELETE ON gate_evidence_requirements
  FOR EACH ROW EXECUTE FUNCTION reject_immutable_change();
CREATE TRIGGER review_verdicts_append_only
  BEFORE UPDATE OR DELETE ON review_verdicts
  FOR EACH ROW EXECUTE FUNCTION reject_immutable_change();
CREATE TRIGGER finding_actions_append_only
  BEFORE UPDATE OR DELETE ON finding_actions
  FOR EACH ROW EXECUTE FUNCTION reject_immutable_change();
CREATE TRIGGER deliverable_submissions_append_only
  BEFORE UPDATE OR DELETE ON deliverable_submissions
  FOR EACH ROW EXECUTE FUNCTION reject_immutable_change();

CREATE FUNCTION validate_evidence_target() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
  target_project uuid;
BEGIN
  CASE NEW.target_kind
    WHEN 'deliverable' THEN SELECT project_id INTO target_project FROM deliverables WHERE id=NEW.target_id;
    WHEN 'gate' THEN SELECT project_id INTO target_project FROM review_gates WHERE id=NEW.target_id;
    WHEN 'finding' THEN SELECT project_id INTO target_project FROM review_findings WHERE id=NEW.target_id;
    WHEN 'decision' THEN SELECT project_id INTO target_project FROM decisions WHERE id=NEW.target_id;
  END CASE;
  IF target_project IS NULL OR target_project <> NEW.project_id THEN
    RAISE EXCEPTION 'evidence target %:% is outside project %', NEW.target_kind, NEW.target_id, NEW.project_id
      USING ERRCODE = '23503';
  END IF;
  RETURN NEW;
END;
$$;

CREATE CONSTRAINT TRIGGER evidence_target_scope
  AFTER INSERT ON evidence_links
  DEFERRABLE INITIALLY DEFERRED
  FOR EACH ROW EXECUTE FUNCTION validate_evidence_target();

CREATE FUNCTION protect_review_gate() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF TG_OP = 'DELETE' THEN
    RAISE EXCEPTION 'review gate % cannot be deleted', OLD.id USING ERRCODE = '55000';
  END IF;
  IF NEW.id <> OLD.id OR NEW.organization_id <> OLD.organization_id
      OR NEW.project_id <> OLD.project_id OR NEW.deliverable_id <> OLD.deliverable_id
      OR NEW.name <> OLD.name OR NEW.kind <> OLD.kind OR NEW.hard <> OLD.hard
      OR NEW.independence_required <> OLD.independence_required
      OR NEW.created_by <> OLD.created_by OR NEW.created_at <> OLD.created_at
      OR NEW.version <> OLD.version + 1 THEN
    RAISE EXCEPTION 'review gate stable identity or version was rewritten' USING ERRCODE = '23514';
  END IF;
  IF OLD.hard AND NEW.state = 'waived' THEN
    RAISE EXCEPTION 'hard gate % cannot be waived', OLD.id USING ERRCODE = '23514';
  END IF;
  RETURN NEW;
END;
$$;

CREATE TRIGGER review_gate_guard
  BEFORE UPDATE OR DELETE ON review_gates
  FOR EACH ROW EXECUTE FUNCTION protect_review_gate();

CREATE FUNCTION protect_review_finding() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF TG_OP = 'DELETE' THEN
    RAISE EXCEPTION 'review finding % cannot be deleted', OLD.id USING ERRCODE = '55000';
  END IF;
  IF NEW.id <> OLD.id OR NEW.organization_id <> OLD.organization_id
      OR NEW.project_id <> OLD.project_id OR NEW.deliverable_id <> OLD.deliverable_id
      OR NEW.gate_id <> OLD.gate_id OR NEW.verdict_id <> OLD.verdict_id
      OR NEW.title <> OLD.title OR NEW.detail <> OLD.detail OR NEW.blocking <> OLD.blocking
      OR NEW.created_by <> OLD.created_by OR NEW.created_at <> OLD.created_at
      OR NEW.version <> OLD.version + 1 OR OLD.state <> 'open' OR NEW.state = 'open' THEN
    RAISE EXCEPTION 'review finding history was rewritten' USING ERRCODE = '23514';
  END IF;
  RETURN NEW;
END;
$$;

CREATE TRIGGER review_finding_guard
  BEFORE UPDATE OR DELETE ON review_findings
  FOR EACH ROW EXECUTE FUNCTION protect_review_finding();

CREATE FUNCTION validate_submission_head_history() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM deliverable_submissions submission
    WHERE submission.id=NEW.submission_id AND submission.deliverable_id=NEW.deliverable_id
  ) THEN
    RAISE EXCEPTION 'submission head does not reference its deliverable' USING ERRCODE = '23514';
  END IF;
  RETURN NEW;
END;
$$;

CREATE CONSTRAINT TRIGGER submission_head_history
  AFTER INSERT OR UPDATE ON deliverable_submission_heads
  DEFERRABLE INITIALLY DEFERRED
  FOR EACH ROW EXECUTE FUNCTION validate_submission_head_history();

CREATE INDEX evidence_project_idx ON evidence (project_id, produced_at, id);
CREATE INDEX evidence_links_target_idx ON evidence_links (target_kind, target_id, evidence_id);
CREATE INDEX review_gates_deliverable_idx ON review_gates (deliverable_id, created_at, id);
CREATE INDEX review_verdicts_gate_idx ON review_verdicts (gate_id, created_at, id);
CREATE INDEX review_findings_deliverable_idx ON review_findings (deliverable_id, state, created_at, id);
CREATE INDEX finding_actions_finding_idx ON finding_actions (finding_id, created_at, id);
CREATE INDEX deliverable_submissions_idx ON deliverable_submissions (deliverable_id, created_at, id);

ALTER TABLE projection_replay_runs
  ADD COLUMN review_count bigint CHECK (review_count IS NULL OR review_count >= 0);
ALTER TABLE projection_heads
  ADD COLUMN review_count bigint NOT NULL DEFAULT 0 CHECK (review_count >= 0);

CREATE TABLE replay_review_projections (
  run_id uuid NOT NULL REFERENCES projection_replay_runs(id) ON DELETE CASCADE,
  kind text NOT NULL CHECK (kind IN (
    'evidence','gate','verdict','finding','finding-action','submission','submission-head'
  )),
  projection_id text NOT NULL,
  organization_id uuid NOT NULL,
  project_id uuid NOT NULL,
  projection jsonb NOT NULL CHECK (jsonb_typeof(projection) = 'object'),
  PRIMARY KEY (run_id, kind, project_id, projection_id)
);

GRANT SELECT ON evidence, evidence_links, review_gates, gate_evidence_requirements,
  review_verdicts, review_findings, finding_actions, deliverable_submissions,
  deliverable_submission_heads, replay_review_projections TO workplane_app;
GRANT INSERT ON evidence, evidence_links, gate_evidence_requirements, review_verdicts,
  finding_actions, deliverable_submissions, replay_review_projections TO workplane_app;
GRANT INSERT, UPDATE ON review_gates, review_findings, deliverable_submission_heads TO workplane_app;

INSERT INTO schema_migrations (version) VALUES (7);

COMMIT;
