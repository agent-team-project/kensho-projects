BEGIN;

-- The upgrade is additive. Capture the accepted M2D authority rows before any
-- M2E DDL and fail the transaction if an existing ledger/outbox/checkpoint or
-- public aggregate changes underneath the migration.
CREATE TEMP TABLE m2e_upgrade_baseline ON COMMIT DROP AS
SELECT
  (SELECT count(*) FROM projects) AS projects_count,
  (SELECT count(*) FROM deliverables) AS deliverables_count,
  (SELECT count(*) FROM evidence) AS evidence_count,
  (SELECT count(*) FROM review_gates) AS gates_count,
  (SELECT count(*) FROM domain_events) AS events_count,
  (SELECT encode(digest(COALESCE(string_agg(to_jsonb(event)::text, '' ORDER BY sequence), ''), 'sha256'), 'hex') FROM domain_events event) AS events_digest,
  (SELECT count(*) FROM outbox_records) AS outbox_count,
  (SELECT encode(digest(COALESCE(string_agg(to_jsonb(record)::text, '' ORDER BY event_sequence), ''), 'sha256'), 'hex') FROM outbox_records record) AS outbox_digest,
  (SELECT count(*) FROM consumer_checkpoints) AS checkpoints_count,
  (SELECT encode(digest(COALESCE(string_agg(to_jsonb(checkpoint)::text, '' ORDER BY consumer_name), ''), 'sha256'), 'hex') FROM consumer_checkpoints checkpoint) AS checkpoints_digest;

-- Snapshot every accepted pre-M2E relational row, not just the headline
-- aggregates above. The final comparison uses EXCEPT ALL so duplicate-valued
-- rows and cardinality are preserved exactly. schema_migrations is the only
-- intentional old-table change in this transaction.
CREATE TEMP TABLE m2e_accepted_row_baseline (
  relation_name text NOT NULL,
  row_data jsonb NOT NULL
) ON COMMIT DROP;

CREATE TEMP TABLE m2e_accepted_relations ON COMMIT DROP AS
SELECT tablename AS relation_name
FROM pg_tables
WHERE schemaname='public' AND tablename<>'schema_migrations';

DO $$
DECLARE
  relation record;
BEGIN
  FOR relation IN
    SELECT relation_name AS tablename FROM m2e_accepted_relations ORDER BY relation_name
  LOOP
    EXECUTE format(
      'INSERT INTO m2e_accepted_row_baseline (relation_name,row_data) SELECT %L,to_jsonb(row_value) FROM %I row_value',
      relation.tablename, relation.tablename
    );
  END LOOP;
END;
$$;

CREATE TYPE work_item_state AS ENUM ('open', 'in_progress', 'in_review', 'done', 'cancelled');
CREATE TYPE work_item_priority AS ENUM ('low', 'normal', 'high', 'critical');
CREATE TYPE work_dependency_kind AS ENUM ('blocks', 'relates', 'caused-by');

CREATE TABLE work_items (
  id uuid PRIMARY KEY,
  organization_id uuid NOT NULL REFERENCES organizations(id),
  project_id uuid NOT NULL REFERENCES projects(id),
  deliverable_id uuid,
  title text NOT NULL CHECK (length(title) BETWEEN 1 AND 200),
  description text NOT NULL CHECK (length(description) BETWEEN 1 AND 4000),
  state work_item_state NOT NULL DEFAULT 'open',
  priority work_item_priority NOT NULL,
  assignee_id uuid REFERENCES principals(id),
  version bigint NOT NULL DEFAULT 1 CHECK (version >= 1),
  created_by uuid NOT NULL REFERENCES principals(id),
  created_at timestamptz NOT NULL,
  updated_at timestamptz NOT NULL,
  UNIQUE (organization_id, id),
  UNIQUE (project_id, id),
  FOREIGN KEY (organization_id, project_id) REFERENCES projects(organization_id, id),
  FOREIGN KEY (project_id, deliverable_id) REFERENCES deliverables(project_id, id)
);

CREATE TABLE work_item_dependencies (
  id uuid PRIMARY KEY,
  organization_id uuid NOT NULL REFERENCES organizations(id),
  source_work_item_id uuid NOT NULL,
  target_work_item_id uuid NOT NULL,
  kind work_dependency_kind NOT NULL,
  version bigint NOT NULL DEFAULT 1 CHECK (version = 1),
  created_by uuid NOT NULL REFERENCES principals(id),
  created_at timestamptz NOT NULL,
  CHECK (source_work_item_id <> target_work_item_id),
  UNIQUE (source_work_item_id, target_work_item_id),
  FOREIGN KEY (organization_id, source_work_item_id) REFERENCES work_items(organization_id, id),
  FOREIGN KEY (organization_id, target_work_item_id) REFERENCES work_items(organization_id, id)
);

CREATE FUNCTION protect_work_item() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF TG_OP = 'DELETE' THEN
    RAISE EXCEPTION 'work item % cannot be deleted', OLD.id USING ERRCODE = '55000';
  END IF;
  IF NEW.id <> OLD.id OR NEW.organization_id <> OLD.organization_id
      OR NEW.project_id <> OLD.project_id OR NEW.created_by <> OLD.created_by
      OR NEW.created_at <> OLD.created_at OR NEW.version <> OLD.version + 1 THEN
    RAISE EXCEPTION 'work item stable identity or version was rewritten' USING ERRCODE = '23514';
  END IF;
  RETURN NEW;
END;
$$;

CREATE TRIGGER work_item_guard
  BEFORE UPDATE OR DELETE ON work_items
  FOR EACH ROW EXECUTE FUNCTION protect_work_item();

CREATE FUNCTION validate_work_item_assignee() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.assignee_id IS NOT NULL AND NOT EXISTS (
    SELECT 1 FROM principals principal
    JOIN organization_memberships membership
      ON membership.principal_id=principal.id AND membership.organization_id=NEW.organization_id
    WHERE principal.id=NEW.assignee_id AND principal.status='active'
  ) THEN
    RAISE EXCEPTION 'work item assignee is not active in its organization' USING ERRCODE = '23503';
  END IF;
  RETURN NEW;
END;
$$;

CREATE CONSTRAINT TRIGGER work_item_assignee_scope
  AFTER INSERT OR UPDATE OF assignee_id, organization_id ON work_items
  DEFERRABLE INITIALLY DEFERRED
  FOR EACH ROW EXECUTE FUNCTION validate_work_item_assignee();

-- This database guard and the application command both take the same
-- organization-scoped transaction lock. Inverse concurrent inserts therefore
-- cannot each observe an acyclic snapshot and commit a cycle.
CREATE FUNCTION reject_blocking_dependency_cycle() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
  closes_cycle boolean;
BEGIN
  IF NEW.kind <> 'blocks' THEN
    RETURN NEW;
  END IF;
  PERFORM pg_advisory_xact_lock(hashtextextended(NEW.organization_id::text || ':work-dependency-graph', 0));
  WITH RECURSIVE reachable(id) AS (
    SELECT dependency.target_work_item_id
    FROM work_item_dependencies dependency
    WHERE dependency.organization_id=NEW.organization_id
      AND dependency.kind='blocks'
      AND dependency.source_work_item_id=NEW.target_work_item_id
    UNION
    SELECT dependency.target_work_item_id
    FROM work_item_dependencies dependency
    JOIN reachable ON dependency.source_work_item_id=reachable.id
    WHERE dependency.organization_id=NEW.organization_id AND dependency.kind='blocks'
  )
  SELECT EXISTS (SELECT 1 FROM reachable WHERE id=NEW.source_work_item_id) INTO closes_cycle;
  IF closes_cycle THEN
    RAISE EXCEPTION 'dependency_cycle' USING ERRCODE = 'P0001';
  END IF;
  RETURN NEW;
END;
$$;

CREATE TRIGGER blocking_dependency_dag
  BEFORE INSERT ON work_item_dependencies
  FOR EACH ROW EXECUTE FUNCTION reject_blocking_dependency_cycle();

CREATE FUNCTION protect_work_item_dependency() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  RAISE EXCEPTION 'work item dependency rows are immutable; remove and recreate the edge'
    USING ERRCODE = '55000';
END;
$$;

CREATE TRIGGER work_item_dependency_update_guard
  BEFORE UPDATE ON work_item_dependencies
  FOR EACH ROW EXECUTE FUNCTION protect_work_item_dependency();

CREATE INDEX work_items_project_idx ON work_items (project_id, created_at, id);
CREATE INDEX work_items_assignee_idx ON work_items (organization_id, assignee_id, state, id);
CREATE INDEX work_dependencies_source_idx ON work_item_dependencies (organization_id, source_work_item_id, kind, target_work_item_id);
CREATE INDEX work_dependencies_target_idx ON work_item_dependencies (organization_id, target_work_item_id, kind, source_work_item_id);

ALTER TABLE projection_replay_runs
  ADD COLUMN work_count bigint CHECK (work_count IS NULL OR work_count >= 0);
ALTER TABLE projection_heads
  ADD COLUMN work_count bigint NOT NULL DEFAULT 0 CHECK (work_count >= 0);

CREATE TABLE replay_work_projections (
  run_id uuid NOT NULL REFERENCES projection_replay_runs(id) ON DELETE CASCADE,
  kind text NOT NULL CHECK (kind IN ('work-item', 'dependency')),
  projection_id uuid NOT NULL,
  organization_id uuid NOT NULL,
  project_id uuid,
  projection jsonb NOT NULL CHECK (jsonb_typeof(projection) = 'object'),
  PRIMARY KEY (run_id, kind, projection_id)
);

GRANT SELECT ON work_items, work_item_dependencies, replay_work_projections TO workplane_app;
GRANT INSERT, UPDATE ON work_items TO workplane_app;
-- UPDATE privilege is required by PostgreSQL for SELECT ... FOR UPDATE. The
-- immutable-row trigger still rejects every actual edge update.
GRANT INSERT, DELETE, UPDATE ON work_item_dependencies TO workplane_app;
GRANT INSERT ON replay_work_projections TO workplane_app;

DO $$
DECLARE
  baseline m2e_upgrade_baseline%ROWTYPE;
BEGIN
  SELECT * INTO baseline FROM m2e_upgrade_baseline;
  IF baseline.projects_count <> (SELECT count(*) FROM projects)
      OR baseline.deliverables_count <> (SELECT count(*) FROM deliverables)
      OR baseline.evidence_count <> (SELECT count(*) FROM evidence)
      OR baseline.gates_count <> (SELECT count(*) FROM review_gates)
      OR baseline.events_count <> (SELECT count(*) FROM domain_events)
      OR baseline.events_digest <> (SELECT encode(digest(COALESCE(string_agg(to_jsonb(event)::text, '' ORDER BY sequence), ''), 'sha256'), 'hex') FROM domain_events event)
      OR baseline.outbox_count <> (SELECT count(*) FROM outbox_records)
      OR baseline.outbox_digest <> (SELECT encode(digest(COALESCE(string_agg(to_jsonb(record)::text, '' ORDER BY event_sequence), ''), 'sha256'), 'hex') FROM outbox_records record)
      OR baseline.checkpoints_count <> (SELECT count(*) FROM consumer_checkpoints)
      OR baseline.checkpoints_digest <> (SELECT encode(digest(COALESCE(string_agg(to_jsonb(checkpoint)::text, '' ORDER BY consumer_name), ''), 'sha256'), 'hex') FROM consumer_checkpoints checkpoint) THEN
    RAISE EXCEPTION 'M2D-to-M2E upgrade changed accepted authority rows' USING ERRCODE = '23514';
  END IF;
END;
$$;

DO $$
DECLARE
  relation record;
  changed boolean;
  after_expression text;
BEGIN
  FOR relation IN
    SELECT relation_name FROM m2e_accepted_relations ORDER BY relation_name
  LOOP
    after_expression := 'to_jsonb(row_value)';
    IF relation.relation_name IN ('projection_heads','projection_replay_runs') THEN
      -- work_count is the sole additive column on an accepted pre-M2E table;
      -- compare the exact pre-upgrade row shape.
      after_expression := 'to_jsonb(row_value) - ''work_count''';
    END IF;
    EXECUTE format(
      'SELECT EXISTS (
         (SELECT row_data FROM m2e_accepted_row_baseline WHERE relation_name=%L
          EXCEPT ALL SELECT %s FROM %I row_value)
         UNION ALL
         (SELECT %s FROM %I row_value
          EXCEPT ALL SELECT row_data FROM m2e_accepted_row_baseline WHERE relation_name=%L)
       )',
      relation.relation_name, after_expression, relation.relation_name,
      after_expression, relation.relation_name, relation.relation_name
    ) INTO changed;
    IF changed THEN
      RAISE EXCEPTION 'M2E migration rewrote accepted rows in %', relation.relation_name;
    END IF;
  END LOOP;
END;
$$;

INSERT INTO schema_migrations (version) VALUES (8);

COMMIT;
