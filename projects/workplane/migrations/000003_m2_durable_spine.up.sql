BEGIN;

CREATE EXTENSION IF NOT EXISTS pgcrypto;

ALTER TABLE domain_events DROP CONSTRAINT domain_events_schema_version_check;
ALTER TABLE domain_events ADD CONSTRAINT domain_events_schema_version_check CHECK (schema_version >= 1);

CREATE TABLE outbox_records (
  id uuid PRIMARY KEY,
  event_sequence bigint NOT NULL UNIQUE REFERENCES domain_events(sequence),
  event_id uuid NOT NULL UNIQUE REFERENCES domain_events(event_id),
  topic text NOT NULL CHECK (length(topic) BETWEEN 1 AND 200),
  payload jsonb NOT NULL CHECK (jsonb_typeof(payload) = 'object'),
  payload_sha256 bytea NOT NULL CHECK (octet_length(payload_sha256) = 32),
  available_at timestamptz NOT NULL,
  created_at timestamptz NOT NULL
);

CREATE TABLE outbox_consumers (
  name text PRIMARY KEY CHECK (name ~ '^[a-z][a-z0-9._-]{0,99}$'),
  enabled boolean NOT NULL DEFAULT true,
  created_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE outbox_delivery_state (
  consumer_name text NOT NULL REFERENCES outbox_consumers(name),
  event_id uuid NOT NULL REFERENCES outbox_records(event_id),
  attempts integer NOT NULL DEFAULT 0 CHECK (attempts >= 0),
  lease_owner text,
  lease_until timestamptz,
  delivered_at timestamptz,
  updated_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
  PRIMARY KEY (consumer_name, event_id),
  CHECK ((lease_owner IS NULL) = (lease_until IS NULL))
);

CREATE TABLE outbox_delivery_attempts (
  sequence bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  consumer_name text NOT NULL REFERENCES outbox_consumers(name),
  event_id uuid NOT NULL REFERENCES outbox_records(event_id),
  attempt integer NOT NULL CHECK (attempt > 0),
  lease_owner text NOT NULL,
  published_at timestamptz NOT NULL,
  duplicate boolean NOT NULL,
  payload_sha256 bytea NOT NULL CHECK (octet_length(payload_sha256) = 32)
);

CREATE TABLE consumer_deliveries (
  consumer_name text NOT NULL REFERENCES outbox_consumers(name),
  event_id uuid NOT NULL REFERENCES outbox_records(event_id),
  event_sequence bigint NOT NULL REFERENCES outbox_records(event_sequence),
  payload_sha256 bytea NOT NULL CHECK (octet_length(payload_sha256) = 32),
  first_published_at timestamptz NOT NULL,
  PRIMARY KEY (consumer_name, event_id),
  UNIQUE (consumer_name, event_sequence)
);

CREATE TABLE consumer_checkpoints (
  consumer_name text PRIMARY KEY REFERENCES outbox_consumers(name),
  last_sequence bigint NOT NULL DEFAULT 0 CHECK (last_sequence >= 0),
  last_event_id uuid,
  updated_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CHECK ((last_sequence = 0 AND last_event_id IS NULL) OR (last_sequence > 0 AND last_event_id IS NOT NULL))
);

CREATE TYPE replay_status AS ENUM ('building', 'succeeded', 'failed');

CREATE TABLE projection_replay_runs (
  id uuid PRIMARY KEY,
  status replay_status NOT NULL,
  started_at timestamptz NOT NULL,
  finished_at timestamptz,
  last_sequence bigint NOT NULL DEFAULT 0 CHECK (last_sequence >= 0),
  failed_sequence bigint,
  failed_event_id uuid,
  failure_code text,
  failure_detail text,
  projects_count bigint,
  decisions_count bigint,
  activity_count bigint,
  live_checksum text CHECK (live_checksum IS NULL OR live_checksum ~ '^[0-9a-f]{64}$'),
  rebuilt_checksum text CHECK (rebuilt_checksum IS NULL OR rebuilt_checksum ~ '^[0-9a-f]{64}$')
);

CREATE TABLE replay_project_projections (
  run_id uuid NOT NULL REFERENCES projection_replay_runs(id) ON DELETE CASCADE,
  organization_id uuid NOT NULL,
  project_id uuid NOT NULL,
  projection jsonb NOT NULL CHECK (jsonb_typeof(projection) = 'object'),
  PRIMARY KEY (run_id, project_id)
);

CREATE TABLE replay_decision_projections (
  run_id uuid NOT NULL REFERENCES projection_replay_runs(id) ON DELETE CASCADE,
  organization_id uuid NOT NULL,
  decision_id uuid NOT NULL,
  project_id uuid NOT NULL,
  projection jsonb NOT NULL CHECK (jsonb_typeof(projection) = 'object'),
  PRIMARY KEY (run_id, decision_id)
);

CREATE TABLE replay_activity_projections (
  run_id uuid NOT NULL REFERENCES projection_replay_runs(id) ON DELETE CASCADE,
  sequence bigint NOT NULL,
  event_id uuid NOT NULL,
  projection jsonb NOT NULL CHECK (jsonb_typeof(projection) = 'object'),
  PRIMARY KEY (run_id, sequence),
  UNIQUE (run_id, event_id)
);

CREATE TABLE projection_heads (
  name text PRIMARY KEY CHECK (name ~ '^[a-z][a-z0-9._-]{0,99}$'),
  run_id uuid NOT NULL REFERENCES projection_replay_runs(id),
  last_sequence bigint NOT NULL CHECK (last_sequence >= 0),
  checksum text NOT NULL CHECK (checksum ~ '^[0-9a-f]{64}$'),
  projects_count bigint NOT NULL CHECK (projects_count >= 0),
  decisions_count bigint NOT NULL CHECK (decisions_count >= 0),
  activity_count bigint NOT NULL CHECK (activity_count >= 0),
  updated_at timestamptz NOT NULL
);

CREATE INDEX outbox_claim_idx ON outbox_delivery_state (consumer_name, delivered_at, lease_until);
CREATE INDEX outbox_event_sequence_idx ON outbox_records (event_sequence);
CREATE INDEX delivery_attempt_event_idx ON outbox_delivery_attempts (consumer_name, event_id, sequence);
CREATE INDEX replay_run_status_idx ON projection_replay_runs (status, started_at);

CREATE FUNCTION seed_outbox_delivery_state() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  INSERT INTO outbox_delivery_state (consumer_name, event_id)
  SELECT name, NEW.event_id FROM outbox_consumers WHERE enabled;
  RETURN NEW;
END;
$$;

CREATE TRIGGER outbox_seed_consumers
  AFTER INSERT ON outbox_records
  FOR EACH ROW EXECUTE FUNCTION seed_outbox_delivery_state();

CREATE FUNCTION domain_event_outbox_envelope(source_event domain_events) RETURNS jsonb
LANGUAGE sql
IMMUTABLE
PARALLEL SAFE
SET search_path = pg_catalog, public
AS $$
  SELECT jsonb_build_object(
    'sequence', ($1).sequence,
    'event_id', ($1).event_id,
    'event_type', ($1).event_type,
    'schema_version', ($1).schema_version,
    'organization_id', ($1).organization_id,
    'aggregate_type', ($1).aggregate_type,
    'aggregate_id', ($1).aggregate_id,
    'aggregate_version', ($1).aggregate_version,
    'command_id', ($1).command_id,
    'request_id', ($1).request_id,
    'actor_kind', ($1).actor_kind,
    'actor_id', ($1).actor_id,
    'principal_id', ($1).principal_id,
    'occurred_at', to_char(($1).occurred_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.US"Z"'),
    'payload', ($1).payload
  )
$$;

CREATE FUNCTION create_event_outbox() RETURNS trigger
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public
AS $$
DECLARE
  envelope jsonb;
BEGIN
  envelope := domain_event_outbox_envelope(NEW);
  INSERT INTO outbox_records
    (id,event_sequence,event_id,topic,payload,payload_sha256,available_at,created_at)
  VALUES
    (NEW.event_id,NEW.sequence,NEW.event_id,'domain-events',envelope,
     digest(convert_to(envelope::text,'UTF8'),'sha256'),NEW.occurred_at,NEW.occurred_at);
  RETURN NEW;
END;
$$;

CREATE TRIGGER domain_event_creates_outbox
  AFTER INSERT ON domain_events
  FOR EACH ROW EXECUTE FUNCTION create_event_outbox();

CREATE TRIGGER outbox_records_immutable
  BEFORE UPDATE OR DELETE ON outbox_records
  FOR EACH ROW EXECUTE FUNCTION reject_immutable_change();

CREATE FUNCTION enforce_monotonic_checkpoint() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.last_sequence < OLD.last_sequence THEN
    RAISE EXCEPTION 'consumer checkpoint cannot move backwards from % to %', OLD.last_sequence, NEW.last_sequence
      USING ERRCODE = '55000';
  END IF;
  IF NEW.last_sequence = OLD.last_sequence AND NEW.last_event_id IS DISTINCT FROM OLD.last_event_id THEN
    RAISE EXCEPTION 'consumer checkpoint event identity cannot change at sequence %', OLD.last_sequence
      USING ERRCODE = '55000';
  END IF;
  RETURN NEW;
END;
$$;

CREATE TRIGGER consumer_checkpoint_monotonic
  BEFORE UPDATE ON consumer_checkpoints
  FOR EACH ROW EXECUTE FUNCTION enforce_monotonic_checkpoint();

INSERT INTO outbox_consumers (name) VALUES ('projection-v1');
INSERT INTO consumer_checkpoints (consumer_name) VALUES ('projection-v1');

WITH existing_events AS MATERIALIZED (
  SELECT event.*, domain_event_outbox_envelope(event) AS envelope
  FROM domain_events AS event
)
INSERT INTO outbox_records
  (id,event_sequence,event_id,topic,payload,payload_sha256,available_at,created_at)
SELECT event_id,sequence,event_id,'domain-events',envelope,
  digest(convert_to(envelope::text,'UTF8'),'sha256'),occurred_at,occurred_at
FROM existing_events
ORDER BY sequence;

DO $$
DECLARE
  event_count bigint;
  outbox_count bigint;
  state_count bigint;
BEGIN
  SELECT count(*) INTO event_count FROM domain_events;
  SELECT count(*) INTO outbox_count FROM outbox_records;
  SELECT count(*) INTO state_count FROM outbox_delivery_state WHERE consumer_name = 'projection-v1';

  IF event_count <> outbox_count OR event_count <> state_count THEN
    RAISE EXCEPTION 'M1 outbox backfill coverage mismatch: events=%, outbox=%, projection-v1 state=%',
      event_count, outbox_count, state_count USING ERRCODE = '23514';
  END IF;

  IF EXISTS (
    SELECT 1
    FROM domain_events AS event
    LEFT JOIN outbox_records AS record
      ON record.event_sequence = event.sequence AND record.event_id = event.event_id
    LEFT JOIN outbox_delivery_state AS state
      ON state.consumer_name = 'projection-v1' AND state.event_id = event.event_id
    WHERE record.event_id IS NULL OR state.event_id IS NULL
      OR record.id <> event.event_id
      OR record.topic <> 'domain-events'
      OR record.payload IS DISTINCT FROM domain_event_outbox_envelope(event)
      OR record.payload_sha256 IS DISTINCT FROM digest(convert_to(record.payload::text,'UTF8'),'sha256')
      OR record.available_at IS DISTINCT FROM event.occurred_at
      OR record.created_at IS DISTINCT FROM event.occurred_at
      OR state.attempts <> 0 OR state.lease_owner IS NOT NULL OR state.lease_until IS NOT NULL
      OR state.delivered_at IS NOT NULL
  ) THEN
    RAISE EXCEPTION 'M1 outbox backfill failed canonical event/outbox/state validation'
      USING ERRCODE = '23514';
  END IF;
END;
$$;

DO $$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'workplane_app') THEN
    CREATE ROLE workplane_app LOGIN PASSWORD 'workplane-app-local-only' NOSUPERUSER NOCREATEDB NOCREATEROLE NOINHERIT;
  END IF;
END;
$$;

GRANT CONNECT ON DATABASE workplane TO workplane_app;
GRANT USAGE ON SCHEMA public TO workplane_app;
GRANT SELECT ON ALL TABLES IN SCHEMA public TO workplane_app;
GRANT INSERT, UPDATE ON organizations, principals, organization_memberships, human_credentials,
  human_sessions, agent_tokens, projects TO workplane_app;
GRANT INSERT ON decisions, domain_events, idempotency_results, security_audit_events TO workplane_app;
GRANT INSERT, UPDATE ON outbox_consumers, outbox_delivery_state, consumer_checkpoints,
  projection_replay_runs, projection_heads TO workplane_app;
GRANT INSERT ON outbox_delivery_attempts, consumer_deliveries, replay_project_projections,
  replay_decision_projections, replay_activity_projections TO workplane_app;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO workplane_app;

INSERT INTO schema_migrations (version) VALUES (3);

COMMIT;
