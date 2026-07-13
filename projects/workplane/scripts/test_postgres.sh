#!/bin/sh
set -eu

cd "$(dirname "$0")/.."
project="workplane_m0_${$}"

cleanup() {
  docker compose --project-name "$project" down --volumes --remove-orphans >/dev/null 2>&1 || true
}
trap cleanup EXIT INT TERM

expected_digest="$(cut -d ' ' -f 1 fixtures/reference-organization.sha256)"
actual_digest="$(shasum -a 256 fixtures/reference-organization.json | cut -d ' ' -f 1)"
test "$expected_digest" = "$actual_digest"

docker compose --project-name "$project" up -d --wait postgres
for migration in migrations/*.up.sql; do
  docker compose --project-name "$project" exec -T postgres \
    psql -v ON_ERROR_STOP=1 -U workplane -d workplane < "$migration"
done
docker compose --project-name "$project" exec -T postgres \
  psql -v ON_ERROR_STOP=1 -U workplane -d workplane < fixtures/reference-organization.sql

counts="$(docker compose --project-name "$project" exec -T postgres \
  psql -At -U workplane -d workplane -c \
  'SELECT (SELECT count(*) FROM organizations) || '\''|'\'' || (SELECT count(*) FROM principals) || '\''|'\'' || (SELECT count(*) FROM organization_memberships);')"
test "$counts" = "1|3|3"

shape="$(docker compose --project-name "$project" exec -T postgres \
  psql -At -U workplane -d workplane -c \
  "SELECT slug || '|' || name FROM organizations ORDER BY id;")"
test "$shape" = "reference|Reference Organization"

durable_shape="$(docker compose --project-name "$project" exec -T postgres \
  psql -At -U workplane -d workplane -c \
  "SELECT max(version) || '|' || (SELECT count(*) FROM outbox_consumers) || '|' ||
    (SELECT count(*) FROM projection_heads) || '|' ||
    COALESCE(to_regclass('public.realtime_retention')::text,'absent') || '|' ||
    COALESCE(to_regclass('public.project_memberships')::text,'absent') FROM schema_migrations;")"
if [ "$durable_shape" != "8|2|0|realtime_retention|project_memberships" ]; then
  echo "unexpected durable schema shape: $durable_shape" >&2
  exit 1
fi

work_shape="$(docker compose --project-name "$project" exec -T postgres \
  psql -At -U workplane -d workplane -c \
  "SELECT to_regclass('public.work_items')::text || '|' ||
    to_regclass('public.work_item_dependencies')::text || '|' ||
    to_regclass('public.replay_work_projections')::text || '|' ||
    COALESCE((SELECT work_count FROM projection_heads WHERE name='m1-canonical')::text,'no-head');")"
if [ "$work_shape" != "work_items|work_item_dependencies|replay_work_projections|no-head" ]; then
  echo "unexpected M2E work schema shape: $work_shape" >&2
  exit 1
fi

review_shape="$(docker compose --project-name "$project" exec -T postgres \
  psql -At -U workplane -d workplane -c \
  "SELECT to_regclass('public.evidence')::text || '|' ||
    to_regclass('public.review_gates')::text || '|' ||
    to_regclass('public.review_verdicts')::text || '|' ||
    to_regclass('public.review_findings')::text || '|' ||
    to_regclass('public.deliverable_submissions')::text || '|' ||
    to_regclass('public.replay_review_projections')::text;")"
if [ "$review_shape" != "evidence|review_gates|review_verdicts|review_findings|deliverable_submissions|replay_review_projections" ]; then
  echo "unexpected M2D review schema shape: $review_shape" >&2
  exit 1
fi

planning_shape="$(docker compose --project-name "$project" exec -T postgres \
  psql -At -U workplane -d workplane -c \
  "SELECT to_regclass('public.deliverables')::text || '|' ||
    to_regclass('public.forecasts')::text || '|' ||
    to_regclass('public.forecast_heads')::text || '|' ||
    to_regclass('public.project_targets')::text || '|' ||
    to_regclass('public.project_deadlines')::text || '|' ||
    to_regclass('public.replay_planning_projections')::text;")"
if [ "$planning_shape" != "deliverables|forecasts|forecast_heads|project_targets|project_deadlines|replay_planning_projections" ]; then
  echo "unexpected M2C planning schema shape: $planning_shape" >&2
  exit 1
fi

commit_horizon="$(docker compose --project-name "$project" exec -T postgres \
  psql -At -U workplane -d workplane -c \
  "SELECT to_regprocedure('lock_domain_event_commit_horizon()')::text || '|' ||
    has_function_privilege('workplane_app','lock_domain_event_commit_horizon()','EXECUTE');")"
if [ "$commit_horizon" != "lock_domain_event_commit_horizon()|true" ]; then
  echo "unexpected checkpoint commit-horizon authority: $commit_horizon" >&2
  exit 1
fi

app_role="$(docker compose --project-name "$project" exec -T postgres \
  psql -At -U workplane -d workplane -c \
  "SELECT rolname || '|' || rolsuper FROM pg_roles WHERE rolname='workplane_app';")"
if [ "$app_role" != "workplane_app|false" ]; then
  echo "unexpected application role: $app_role" >&2
  exit 1
fi

printf '%s\n' "PostgreSQL migration, durable/realtime/planning/review/work schema, least-privilege role, and fixture passed: counts=$counts sha256=$actual_digest"
