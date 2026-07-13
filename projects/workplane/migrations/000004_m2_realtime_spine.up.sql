BEGIN;

CREATE TYPE project_visibility AS ENUM ('organization', 'private');

ALTER TABLE projects
  ADD COLUMN visibility project_visibility NOT NULL DEFAULT 'organization';

CREATE TABLE project_memberships (
  project_id uuid NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  principal_id uuid NOT NULL REFERENCES principals(id),
  role text NOT NULL CHECK (role IN ('owner', 'contributor', 'reviewer', 'observer')),
  created_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
  PRIMARY KEY (project_id, principal_id)
);

INSERT INTO project_memberships (project_id, principal_id, role, created_at)
SELECT id, created_by, 'owner', created_at FROM projects
ON CONFLICT DO NOTHING;

CREATE FUNCTION seed_project_creator_membership() RETURNS trigger
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public
AS $$
BEGIN
  INSERT INTO project_memberships (project_id, principal_id, role, created_at)
  VALUES (NEW.id, NEW.created_by, 'owner', NEW.created_at)
  ON CONFLICT DO NOTHING;
  RETURN NEW;
END;
$$;

CREATE TRIGGER project_seeds_creator_membership
  AFTER INSERT ON projects
  FOR EACH ROW EXECUTE FUNCTION seed_project_creator_membership();

-- The immutable event ledger is retained indefinitely. This table advances only
-- the public transport's resumable window, allowing a stale cursor to fail with
-- an explicit snapshot-required outcome without deleting authoritative history.
CREATE TABLE realtime_retention (
  organization_id uuid PRIMARY KEY REFERENCES organizations(id) ON DELETE CASCADE,
  minimum_cursor_sequence bigint NOT NULL DEFAULT 0 CHECK (minimum_cursor_sequence >= 0),
  updated_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP
);

INSERT INTO realtime_retention (organization_id)
SELECT id FROM organizations ON CONFLICT DO NOTHING;

CREATE FUNCTION seed_realtime_retention() RETURNS trigger
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public
AS $$
BEGIN
  INSERT INTO realtime_retention (organization_id) VALUES (NEW.id)
  ON CONFLICT DO NOTHING;
  RETURN NEW;
END;
$$;

CREATE TRIGGER organization_seeds_realtime_retention
  AFTER INSERT ON organizations
  FOR EACH ROW EXECUTE FUNCTION seed_realtime_retention();

INSERT INTO outbox_consumers (name) VALUES ('realtime-v1')
ON CONFLICT (name) DO UPDATE SET enabled = true;

INSERT INTO outbox_delivery_state (consumer_name, event_id)
SELECT 'realtime-v1', event_id FROM outbox_records
ON CONFLICT DO NOTHING;

INSERT INTO consumer_checkpoints (consumer_name) VALUES ('realtime-v1')
ON CONFLICT DO NOTHING;

GRANT SELECT ON project_memberships, realtime_retention TO workplane_app;

INSERT INTO schema_migrations (version) VALUES (4);

COMMIT;
