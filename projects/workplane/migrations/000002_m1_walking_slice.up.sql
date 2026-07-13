BEGIN;

CREATE TABLE human_credentials (
  principal_id uuid PRIMARY KEY REFERENCES principals(id),
  email text NOT NULL UNIQUE CHECK (email = lower(email)),
  password_hash text NOT NULL,
  failed_attempts integer NOT NULL DEFAULT 0 CHECK (failed_attempts >= 0),
  last_failed_at timestamptz
);

CREATE TABLE human_sessions (
  id uuid PRIMARY KEY,
  principal_id uuid NOT NULL REFERENCES principals(id),
  token_hash bytea NOT NULL UNIQUE CHECK (octet_length(token_hash) = 32),
  csrf_hash bytea NOT NULL CHECK (octet_length(csrf_hash) = 32),
  expires_at timestamptz NOT NULL,
  revoked_at timestamptz,
  created_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE agent_tokens (
  id uuid PRIMARY KEY,
  agent_id uuid NOT NULL REFERENCES principals(id),
  organization_id uuid NOT NULL REFERENCES organizations(id),
  token_prefix text NOT NULL UNIQUE CHECK (length(token_prefix) BETWEEN 8 AND 24),
  token_hash bytea NOT NULL UNIQUE CHECK (octet_length(token_hash) = 32),
  scopes text[] NOT NULL CHECK (cardinality(scopes) > 0),
  project_ids uuid[],
  expires_at timestamptz NOT NULL,
  revoked_at timestamptz,
  created_by uuid NOT NULL REFERENCES principals(id),
  created_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
  last_used_at timestamptz
);

CREATE TYPE project_mode AS ENUM ('exploration', 'exploitation');
CREATE TYPE project_state AS ENUM ('proposed', 'active', 'held', 'completed', 'stopped', 'cancelled');

CREATE TABLE projects (
  id uuid PRIMARY KEY,
  organization_id uuid NOT NULL REFERENCES organizations(id),
  title text NOT NULL CHECK (length(title) BETWEEN 1 AND 200),
  outcome text NOT NULL CHECK (length(outcome) BETWEEN 1 AND 2000),
  mode project_mode NOT NULL,
  state project_state NOT NULL,
  version bigint NOT NULL CHECK (version >= 1),
  hypothesis text NOT NULL CHECK (length(hypothesis) BETWEEN 1 AND 2000),
  falsifier text NOT NULL CHECK (length(falsifier) BETWEEN 1 AND 2000),
  decision_criteria jsonb NOT NULL CHECK (jsonb_typeof(decision_criteria) = 'array' AND jsonb_array_length(decision_criteria) > 0),
  experiment_bound text NOT NULL CHECK (length(experiment_bound) > 0),
  created_by uuid NOT NULL REFERENCES principals(id),
  created_at timestamptz NOT NULL,
  updated_at timestamptz NOT NULL,
  UNIQUE (organization_id, id)
);

CREATE TYPE decision_kind AS ENUM ('continue', 'pivot', 'stop', 'promote', 'scope', 'forecast', 'waiver', 'risk-acceptance', 'complete', 'cancel');

CREATE TABLE decisions (
  id uuid PRIMARY KEY,
  organization_id uuid NOT NULL REFERENCES organizations(id),
  project_id uuid NOT NULL REFERENCES projects(id),
  kind decision_kind NOT NULL,
  question text NOT NULL CHECK (length(question) > 0),
  choice text NOT NULL CHECK (length(choice) > 0),
  alternatives jsonb NOT NULL CHECK (jsonb_typeof(alternatives) = 'array'),
  rationale text NOT NULL CHECK (length(rationale) > 0),
  evidence jsonb NOT NULL CHECK (jsonb_typeof(evidence) = 'array'),
  consequences jsonb NOT NULL CHECK (jsonb_typeof(consequences) = 'array'),
  actor_id uuid NOT NULL REFERENCES principals(id),
  recorded_at timestamptz NOT NULL
);

CREATE TABLE domain_events (
  sequence bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  event_id uuid NOT NULL UNIQUE,
  organization_id uuid NOT NULL REFERENCES organizations(id),
  aggregate_type text NOT NULL,
  aggregate_id uuid NOT NULL,
  aggregate_version bigint NOT NULL CHECK (aggregate_version >= 1),
  event_type text NOT NULL,
  schema_version smallint NOT NULL DEFAULT 1 CHECK (schema_version = 1),
  actor_kind principal_kind NOT NULL,
  actor_id uuid NOT NULL REFERENCES principals(id),
  principal_id uuid REFERENCES principals(id),
  command_id uuid NOT NULL,
  request_id text NOT NULL,
  occurred_at timestamptz NOT NULL,
  payload jsonb NOT NULL,
  UNIQUE (aggregate_type, aggregate_id, aggregate_version),
  CHECK ((actor_kind = 'human' AND principal_id IS NULL) OR (actor_kind = 'agent' AND principal_id IS NOT NULL))
);

CREATE TABLE idempotency_results (
  organization_id uuid NOT NULL REFERENCES organizations(id),
  actor_id uuid NOT NULL REFERENCES principals(id),
  operation_id text NOT NULL,
  idempotency_key text NOT NULL CHECK (length(idempotency_key) BETWEEN 16 AND 128),
  request_hash bytea NOT NULL CHECK (octet_length(request_hash) = 32),
  response_status integer NOT NULL CHECK (response_status BETWEEN 200 AND 299),
  response_body bytea NOT NULL,
  response_etag text,
  response_request_id text NOT NULL,
  event_ids uuid[] NOT NULL,
  created_at timestamptz NOT NULL,
  PRIMARY KEY (organization_id, actor_id, operation_id, idempotency_key)
);

CREATE TABLE security_audit_events (
  sequence bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  event_id uuid NOT NULL UNIQUE,
  category text NOT NULL,
  actor_id uuid REFERENCES principals(id),
  organization_id uuid REFERENCES organizations(id),
  request_id text NOT NULL,
  occurred_at timestamptz NOT NULL,
  detail jsonb NOT NULL
);

CREATE INDEX projects_organization_idx ON projects (organization_id, created_at, id);
CREATE INDEX events_project_commit_idx ON domain_events (organization_id, aggregate_id, sequence);
CREATE INDEX sessions_active_idx ON human_sessions (token_hash, expires_at) WHERE revoked_at IS NULL;
CREATE INDEX agent_tokens_active_idx ON agent_tokens (token_prefix, expires_at) WHERE revoked_at IS NULL;

CREATE FUNCTION reject_immutable_change() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  RAISE EXCEPTION '% is append-only', TG_TABLE_NAME USING ERRCODE = '55000';
END;
$$;

CREATE TRIGGER domain_events_append_only
  BEFORE UPDATE OR DELETE ON domain_events
  FOR EACH ROW EXECUTE FUNCTION reject_immutable_change();
CREATE TRIGGER decisions_append_only
  BEFORE UPDATE OR DELETE ON decisions
  FOR EACH ROW EXECUTE FUNCTION reject_immutable_change();
CREATE TRIGGER security_audit_append_only
  BEFORE UPDATE OR DELETE ON security_audit_events
  FOR EACH ROW EXECUTE FUNCTION reject_immutable_change();

INSERT INTO schema_migrations (version) VALUES (2);

COMMIT;
