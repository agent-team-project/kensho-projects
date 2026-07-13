#!/bin/sh
set -eu

cd "$(dirname "$0")/.."
artifact_root="${WORKPLANE_M1_EVIDENCE_DIR:-$PWD/target/agent-evidence/m1}"

cleanup() {
  docker compose down --volumes --remove-orphans >/dev/null 2>&1 || true
}
trap cleanup EXIT INT TERM

docker compose down --volumes --remove-orphans >/dev/null 2>&1 || true
rm -rf "$artifact_root"
mkdir -p "$artifact_root/api" "$artifact_root/browser"
chmod 777 "$artifact_root/api" "$artifact_root/browser"

# Production frontend compilation and both Go executables run without fetching
# dependencies. Bootstrap is the explicit online dependency acquisition step.
npm --prefix web run build
WORKPLANE_FAULT_INJECTION=true docker compose build --no-cache api
if ! WORKPLANE_FAULT_INJECTION=true docker compose up -d --wait; then
  docker compose logs --no-color >&2 || true
  exit 1
fi
docker compose logs migrate --no-color >"$artifact_root/migration.log"

# The internal runtime network must be unable to reach an arbitrary public host.
if docker compose exec -T postgres wget -T 2 -qO- https://example.com >/dev/null 2>&1; then
  echo "offline runtime probe unexpectedly reached the public network" >&2
  exit 1
fi
printf '%s\n' "outbound network probe denied" >"$artifact_root/offline-startup.txt"

docker build --network none --target smoke -t workplane-smoke:m1 . >/dev/null
docker run --rm --network workplane_default \
  -e WORKPLANE_API_BASE=http://api:8080 \
  -e WORKPLANE_DATABASE_URL='postgres://workplane:workplane-local-only@postgres:5432/workplane?sslmode=disable' \
  -v "$artifact_root/api:/evidence" workplane-smoke:m1

docker compose exec -T postgres psql -At -F '|' -U workplane -d workplane -c \
  "SELECT sequence,event_id,event_type,aggregate_version,actor_kind,actor_id,COALESCE(principal_id::text,''),command_id,request_id FROM domain_events ORDER BY sequence" \
  >"$artifact_root/event-rows.txt"

WORKPLANE_BROWSER_EVIDENCE="$artifact_root/browser" scripts/test_browser.sh
.venv/bin/python3 scripts/generate.py --check >"$artifact_root/generated-client-zero-diff.txt"

printf '%s\n' "M1 offline Compose, API parity, PostgreSQL ledger, and browser smoke passed"
