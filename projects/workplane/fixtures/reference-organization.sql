BEGIN;

INSERT INTO organizations (id, slug, name, created_at) VALUES
  ('00000000-0000-4000-8000-000000000001', 'reference', 'Reference Organization', '2026-07-10T00:00:00Z');

INSERT INTO principals (id, kind, display_name, status, human_principal_id, created_at) VALUES
  ('00000000-0000-4000-8000-000000000010', 'human', 'Reference Owner', 'active', NULL, '2026-07-10T00:00:00Z'),
  ('00000000-0000-4000-8000-000000000011', 'human', 'Reference Reviewer', 'active', NULL, '2026-07-10T00:00:00Z'),
  ('00000000-0000-4000-8000-000000000012', 'agent', 'Reference Agent', 'active', '00000000-0000-4000-8000-000000000010', '2026-07-10T00:00:00Z');

INSERT INTO organization_memberships (organization_id, principal_id, role, created_at) VALUES
  ('00000000-0000-4000-8000-000000000001', '00000000-0000-4000-8000-000000000010', 'owner', '2026-07-10T00:00:00Z'),
  ('00000000-0000-4000-8000-000000000001', '00000000-0000-4000-8000-000000000011', 'member', '2026-07-10T00:00:00Z'),
  ('00000000-0000-4000-8000-000000000001', '00000000-0000-4000-8000-000000000012', 'member', '2026-07-10T00:00:00Z');

COMMIT;
