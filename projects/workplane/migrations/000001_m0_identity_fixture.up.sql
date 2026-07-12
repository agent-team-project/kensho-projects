BEGIN;

CREATE TABLE schema_migrations (
  version bigint PRIMARY KEY,
  applied_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TYPE principal_kind AS ENUM ('human', 'agent');
CREATE TYPE principal_status AS ENUM ('active', 'disabled', 'revoked');
CREATE TYPE organization_role AS ENUM ('owner', 'admin', 'member', 'observer');

CREATE TABLE organizations (
  id uuid PRIMARY KEY,
  slug text NOT NULL UNIQUE CHECK (slug ~ '^[a-z][a-z0-9-]{1,62}$'),
  name text NOT NULL CHECK (length(name) BETWEEN 1 AND 200),
  created_at timestamptz NOT NULL
);

CREATE TABLE principals (
  id uuid PRIMARY KEY,
  kind principal_kind NOT NULL,
  display_name text NOT NULL CHECK (length(display_name) BETWEEN 1 AND 200),
  status principal_status NOT NULL,
  human_principal_id uuid REFERENCES principals(id),
  created_at timestamptz NOT NULL,
  CHECK ((kind = 'human' AND human_principal_id IS NULL) OR (kind = 'agent' AND human_principal_id IS NOT NULL))
);

CREATE TABLE organization_memberships (
  organization_id uuid NOT NULL REFERENCES organizations(id),
  principal_id uuid NOT NULL REFERENCES principals(id),
  role organization_role NOT NULL,
  created_at timestamptz NOT NULL,
  PRIMARY KEY (organization_id, principal_id)
);

INSERT INTO schema_migrations (version) VALUES (1);

COMMIT;
