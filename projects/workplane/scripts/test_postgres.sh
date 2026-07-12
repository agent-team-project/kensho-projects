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

printf '%s\n' "PostgreSQL migration and fixture passed: counts=$counts sha256=$actual_digest"
