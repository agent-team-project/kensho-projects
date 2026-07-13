BEGIN;

-- A PostgreSQL identity value is allocated before its transaction commits and
-- is not rolled back. Before a consumer treats every lower missing value as an
-- aborted gap, wait for all transactions that can still commit a domain event.
-- SHARE conflicts with the ROW EXCLUSIVE lock taken before every INSERT while
-- still allowing the two checkpoint consumers to establish the same horizon.
CREATE FUNCTION lock_domain_event_commit_horizon() RETURNS void
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public
AS $$
BEGIN
  LOCK TABLE public.domain_events IN SHARE MODE;
END;
$$;

REVOKE ALL ON FUNCTION lock_domain_event_commit_horizon() FROM PUBLIC;
GRANT EXECUTE ON FUNCTION lock_domain_event_commit_horizon() TO workplane_app;

INSERT INTO schema_migrations (version) VALUES (5);

COMMIT;
