BEGIN;

CREATE TYPE deliverable_state AS ENUM
  ('draft', 'ready', 'submitted', 'bounced', 'accepted', 'waived', 'cancelled');

CREATE TABLE deliverables (
  id uuid PRIMARY KEY,
  organization_id uuid NOT NULL REFERENCES organizations(id),
  project_id uuid NOT NULL REFERENCES projects(id),
  title text NOT NULL CHECK (length(title) BETWEEN 1 AND 200),
  description text NOT NULL CHECK (length(description) BETWEEN 1 AND 4000),
  required boolean NOT NULL,
  weight smallint NOT NULL CHECK (weight BETWEEN 1 AND 1000),
  state deliverable_state NOT NULL,
  acceptance_criteria jsonb NOT NULL CHECK (
    jsonb_typeof(acceptance_criteria) = 'array'
    AND jsonb_array_length(acceptance_criteria) BETWEEN 1 AND 32
  ),
  version bigint NOT NULL CHECK (version >= 1),
  created_by uuid NOT NULL REFERENCES principals(id),
  created_at timestamptz NOT NULL,
  updated_at timestamptz NOT NULL,
  UNIQUE (organization_id, project_id, id),
  UNIQUE (project_id, id),
  FOREIGN KEY (organization_id, project_id) REFERENCES projects(organization_id, id)
);

CREATE FUNCTION protect_immutable_deliverable() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF OLD.state IN ('accepted', 'waived', 'cancelled') AND NEW IS DISTINCT FROM OLD THEN
    RAISE EXCEPTION 'deliverable % in immutable state % cannot be edited', OLD.id, OLD.state
      USING ERRCODE = '23514';
  END IF;
  RETURN NEW;
END;
$$;

CREATE TRIGGER deliverable_immutable_state
  BEFORE UPDATE ON deliverables
  FOR EACH ROW EXECUTE FUNCTION protect_immutable_deliverable();

CREATE FUNCTION text_array_is_unique(items text[]) RETURNS boolean
LANGUAGE sql IMMUTABLE STRICT AS $$
  SELECT cardinality(items) = (SELECT count(DISTINCT value) FROM unnest(items) AS value)
$$;

CREATE TABLE forecasts (
  id uuid PRIMARY KEY,
  organization_id uuid NOT NULL REFERENCES organizations(id),
  project_id uuid NOT NULL REFERENCES projects(id),
  deliverable_id uuid REFERENCES deliverables(id),
  p50_at timestamptz NOT NULL,
  p90_at timestamptz NOT NULL,
  review_after timestamptz NOT NULL,
  basis text NOT NULL CHECK (length(basis) BETWEEN 1 AND 4000),
  assumptions jsonb NOT NULL CHECK (
    jsonb_typeof(assumptions) = 'array'
    AND jsonb_array_length(assumptions) BETWEEN 1 AND 32
  ),
  reason_codes text[] NOT NULL CHECK (
    cardinality(reason_codes) BETWEEN 1 AND 9
    AND reason_codes <@ ARRAY[
      'scope-change','new-evidence','dependency-change','capacity-change',
      'quality-finding','incident','estimate-correction','hold-change','other'
    ]::text[]
    AND text_array_is_unique(reason_codes)
  ),
  impact text NOT NULL CHECK (length(impact) BETWEEN 1 AND 4000),
  supersedes_id uuid REFERENCES forecasts(id),
  created_by uuid NOT NULL REFERENCES principals(id),
  created_at timestamptz NOT NULL,
  CHECK (p50_at <= p90_at),
  CHECK (supersedes_id IS NULL OR supersedes_id <> id),
  UNIQUE (organization_id, project_id, id),
  UNIQUE (project_id, id),
  FOREIGN KEY (organization_id, project_id) REFERENCES projects(organization_id, id),
  FOREIGN KEY (project_id, deliverable_id) REFERENCES deliverables(project_id, id),
  FOREIGN KEY (project_id, supersedes_id) REFERENCES forecasts(project_id, id)
);

CREATE TABLE forecast_heads (
  project_id uuid NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  deliverable_id uuid REFERENCES deliverables(id) ON DELETE CASCADE,
  forecast_id uuid NOT NULL UNIQUE,
  scope_key text GENERATED ALWAYS AS (COALESCE(deliverable_id::text, 'project')) STORED,
  updated_at timestamptz NOT NULL,
  PRIMARY KEY (project_id, scope_key),
  FOREIGN KEY (project_id, forecast_id) REFERENCES forecasts(project_id, id)
);

CREATE FUNCTION validate_forecast_head_scope() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM forecasts forecast
    WHERE forecast.id = NEW.forecast_id
      AND forecast.project_id = NEW.project_id
      AND forecast.deliverable_id IS NOT DISTINCT FROM NEW.deliverable_id
  ) THEN
    RAISE EXCEPTION 'forecast head scope does not match forecast %', NEW.forecast_id
      USING ERRCODE = '23514';
  END IF;
  IF (TG_OP = 'INSERT' AND EXISTS (
      SELECT 1 FROM forecasts forecast
      WHERE forecast.id = NEW.forecast_id AND forecast.supersedes_id IS NOT NULL
    )) OR (TG_OP = 'UPDATE' AND NOT EXISTS (
      SELECT 1 FROM forecasts forecast
      WHERE forecast.id = NEW.forecast_id AND forecast.supersedes_id = OLD.forecast_id
    )) THEN
    RAISE EXCEPTION 'forecast head % does not extend its current history', NEW.forecast_id
      USING ERRCODE = '23514';
  END IF;
  RETURN NEW;
END;
$$;

CREATE CONSTRAINT TRIGGER forecast_head_scope
  AFTER INSERT OR UPDATE ON forecast_heads
  DEFERRABLE INITIALLY DEFERRED
  FOR EACH ROW EXECUTE FUNCTION validate_forecast_head_scope();

CREATE TABLE project_targets (
  id uuid PRIMARY KEY,
  organization_id uuid NOT NULL REFERENCES organizations(id),
  project_id uuid NOT NULL REFERENCES projects(id),
  target_at timestamptz NOT NULL,
  reason text NOT NULL CHECK (length(reason) BETWEEN 1 AND 4000),
  supersedes_id uuid REFERENCES project_targets(id),
  created_by uuid NOT NULL REFERENCES principals(id),
  created_at timestamptz NOT NULL,
  CHECK (supersedes_id IS NULL OR supersedes_id <> id),
  UNIQUE (organization_id, project_id, id),
  UNIQUE (project_id, id),
  FOREIGN KEY (organization_id, project_id) REFERENCES projects(organization_id, id),
  FOREIGN KEY (project_id, supersedes_id) REFERENCES project_targets(project_id, id)
);

CREATE TABLE project_target_heads (
  project_id uuid PRIMARY KEY REFERENCES projects(id) ON DELETE CASCADE,
  target_id uuid NOT NULL UNIQUE,
  updated_at timestamptz NOT NULL,
  FOREIGN KEY (project_id, target_id) REFERENCES project_targets(project_id, id)
);

CREATE FUNCTION validate_target_head_history() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF (TG_OP = 'INSERT' AND EXISTS (
      SELECT 1 FROM project_targets target
      WHERE target.id = NEW.target_id AND target.supersedes_id IS NOT NULL
    )) OR (TG_OP = 'UPDATE' AND NOT EXISTS (
      SELECT 1 FROM project_targets target
      WHERE target.id = NEW.target_id AND target.supersedes_id = OLD.target_id
    )) THEN
    RAISE EXCEPTION 'target head % does not extend its current history', NEW.target_id
      USING ERRCODE = '23514';
  END IF;
  RETURN NEW;
END;
$$;

CREATE CONSTRAINT TRIGGER target_head_history
  AFTER INSERT OR UPDATE ON project_target_heads
  DEFERRABLE INITIALLY DEFERRED
  FOR EACH ROW EXECUTE FUNCTION validate_target_head_history();

CREATE TYPE deadline_source AS ENUM
  ('contract', 'launch-window', 'demonstration', 'regulation', 'other');

CREATE TABLE project_deadlines (
  id uuid PRIMARY KEY,
  organization_id uuid NOT NULL REFERENCES organizations(id),
  project_id uuid NOT NULL REFERENCES projects(id),
  deadline_at timestamptz NOT NULL,
  source deadline_source NOT NULL,
  description text NOT NULL CHECK (length(description) BETWEEN 1 AND 4000),
  supersedes_id uuid REFERENCES project_deadlines(id),
  created_by uuid NOT NULL REFERENCES principals(id),
  created_at timestamptz NOT NULL,
  CHECK (supersedes_id IS NULL OR supersedes_id <> id),
  UNIQUE (organization_id, project_id, id),
  UNIQUE (project_id, id),
  FOREIGN KEY (organization_id, project_id) REFERENCES projects(organization_id, id),
  FOREIGN KEY (project_id, supersedes_id) REFERENCES project_deadlines(project_id, id)
);

CREATE TABLE project_deadline_heads (
  project_id uuid PRIMARY KEY REFERENCES projects(id) ON DELETE CASCADE,
  deadline_id uuid NOT NULL UNIQUE,
  updated_at timestamptz NOT NULL,
  FOREIGN KEY (project_id, deadline_id) REFERENCES project_deadlines(project_id, id)
);

CREATE FUNCTION validate_deadline_head_history() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF (TG_OP = 'INSERT' AND EXISTS (
      SELECT 1 FROM project_deadlines deadline
      WHERE deadline.id = NEW.deadline_id AND deadline.supersedes_id IS NOT NULL
    )) OR (TG_OP = 'UPDATE' AND NOT EXISTS (
      SELECT 1 FROM project_deadlines deadline
      WHERE deadline.id = NEW.deadline_id AND deadline.supersedes_id = OLD.deadline_id
    )) THEN
    RAISE EXCEPTION 'deadline head % does not extend its current history', NEW.deadline_id
      USING ERRCODE = '23514';
  END IF;
  RETURN NEW;
END;
$$;

CREATE CONSTRAINT TRIGGER deadline_head_history
  AFTER INSERT OR UPDATE ON project_deadline_heads
  DEFERRABLE INITIALLY DEFERRED
  FOR EACH ROW EXECUTE FUNCTION validate_deadline_head_history();

CREATE TRIGGER forecasts_append_only
  BEFORE UPDATE OR DELETE ON forecasts
  FOR EACH ROW EXECUTE FUNCTION reject_immutable_change();
CREATE TRIGGER project_targets_append_only
  BEFORE UPDATE OR DELETE ON project_targets
  FOR EACH ROW EXECUTE FUNCTION reject_immutable_change();
CREATE TRIGGER project_deadlines_append_only
  BEFORE UPDATE OR DELETE ON project_deadlines
  FOR EACH ROW EXECUTE FUNCTION reject_immutable_change();

CREATE INDEX deliverables_project_idx ON deliverables (project_id, created_at, id);
CREATE INDEX forecasts_project_idx ON forecasts (project_id, deliverable_id, created_at, id);
CREATE INDEX project_targets_project_idx ON project_targets (project_id, created_at, id);
CREATE INDEX project_deadlines_project_idx ON project_deadlines (project_id, created_at, id);

ALTER TABLE projection_replay_runs
  ADD COLUMN planning_count bigint CHECK (planning_count IS NULL OR planning_count >= 0);
ALTER TABLE projection_heads
  ADD COLUMN planning_count bigint NOT NULL DEFAULT 0 CHECK (planning_count >= 0);

CREATE TABLE replay_planning_projections (
  run_id uuid NOT NULL REFERENCES projection_replay_runs(id) ON DELETE CASCADE,
  kind text NOT NULL CHECK (kind IN ('deliverable','forecast','forecast-head','target','target-head','deadline','deadline-head')),
  projection_id text NOT NULL,
  organization_id uuid NOT NULL,
  project_id uuid NOT NULL,
  projection jsonb NOT NULL CHECK (jsonb_typeof(projection) = 'object'),
  PRIMARY KEY (run_id, kind, project_id, projection_id)
);

GRANT SELECT ON deliverables, forecasts, forecast_heads, project_targets,
  project_target_heads, project_deadlines, project_deadline_heads,
  replay_planning_projections TO workplane_app;
GRANT INSERT, UPDATE ON deliverables, forecast_heads, project_target_heads,
  project_deadline_heads TO workplane_app;
GRANT INSERT ON forecasts, project_targets, project_deadlines,
  replay_planning_projections TO workplane_app;

INSERT INTO schema_migrations (version) VALUES (6);

COMMIT;
