#!/bin/sh
set -eu

cd "$(dirname "$0")/.."
artifact_root="${WORKPLANE_M2B_EVIDENCE_DIR:-$PWD/target/agent-evidence/m2b}"

cleanup() {
  docker compose down --volumes --remove-orphans >/dev/null 2>&1 || true
}
trap cleanup EXIT INT TERM

docker compose down --volumes --remove-orphans >/dev/null 2>&1 || true
rm -rf "$artifact_root"
mkdir -p "$artifact_root"
chmod 777 "$artifact_root"

npm --prefix web run build
WORKPLANE_FAULT_INJECTION=true docker compose build --no-cache api
if ! WORKPLANE_FAULT_INJECTION=true docker compose up -d --wait; then
  docker compose logs --no-color >&2 || true
  exit 1
fi
docker compose logs migrate --no-color >"$artifact_root/migration.log"

docker build --network none --target smoke -t workplane-smoke:m2b . >/dev/null
run_smoke() {
  mode="$1"
  docker run --rm --network workplane_default \
    --entrypoint /workplane-realtime-smoke \
    -e WORKPLANE_API_BASE=http://api:8080 \
    -e WORKPLANE_DATABASE_URL='postgres://workplane:workplane-local-only@postgres:5432/workplane?sslmode=disable' \
    -e WORKPLANE_EVIDENCE_DIR=/evidence \
    -v "$artifact_root:/evidence" \
    workplane-smoke:m2b "$mode"
}

run_smoke prepare-restart
docker compose restart api outbox >/dev/null
if ! WORKPLANE_FAULT_INJECTION=true docker compose up -d --wait; then
  docker compose logs --no-color >&2 || true
  exit 1
fi
run_smoke verify-restart
run_smoke full

for artifact in \
  m2b-restart-resume.json \
  m2b-snapshot-required.json \
  m2b-authority-canaries.json \
  m2b-live-revocation.json \
  m2b-commit-before-publish.json \
  m2b-websocket-backpressure.json \
  m2b-sse-backpressure.json \
  m2b-load-bearing-outcomes.json; do
  test -s "$artifact_root/$artifact"
done

docker compose exec -T postgres psql -At -F '|' -U workplane -d workplane -c \
  "SELECT consumer.name,consumer.enabled,checkpoint.last_sequence,
     (SELECT count(*) FROM consumer_deliveries delivery WHERE delivery.consumer_name=consumer.name)
   FROM outbox_consumers consumer JOIN consumer_checkpoints checkpoint ON checkpoint.consumer_name=consumer.name
   WHERE consumer.name='realtime-v1';" >"$artifact_root/realtime-consumer.txt"
grep -Eq '^realtime-v1\|t\|[1-9][0-9]*\|[1-9][0-9]*$' "$artifact_root/realtime-consumer.txt"

printf '%s\n' "M2B WebSocket/SSE authority, resume, restart, backpressure, revoke, and canary smoke passed"
