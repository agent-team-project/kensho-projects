#!/bin/sh
set -eu

cd "$(dirname "$0")/.."
artifact_root="${WORKPLANE_M2E_EVIDENCE_DIR:-$PWD/target/agent-evidence/m2e}"

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

docker build --network none --target smoke -t workplane-smoke:m2e . >/dev/null
run_smoke() {
  mode="$1"
  if ! docker run --rm --network workplane_default \
    --entrypoint /workplane-work-smoke \
    -e WORKPLANE_API_BASE=http://api:8080 \
    -e WORKPLANE_DATABASE_URL='postgres://workplane:workplane-local-only@postgres:5432/workplane?sslmode=disable' \
    -e WORKPLANE_EVIDENCE_DIR=/evidence \
    -v "$artifact_root:/evidence" \
    workplane-smoke:m2e "$mode"; then
    docker compose logs --no-color >&2 || true
    exit 1
  fi
}

run_smoke exercise

# Prove that projections, graph state, authority fixtures, checkpoint state,
# and exact idempotency results are not process-local.
docker compose restart api outbox >/dev/null
if ! WORKPLANE_FAULT_INJECTION=true docker compose up -d --wait; then
  docker compose logs --no-color >&2 || true
  exit 1
fi
run_smoke verify-restart

for artifact in \
  m2e-lifecycle.json \
  m2e-parity-authority.json \
  m2e-dependencies.json \
  m2e-concurrency.json \
  m2e-batch-atomicity.json \
  m2e-faults.json \
  m2e-current-authority.json \
  m2e-cross-organization.json \
  m2e-realtime-resume.json \
  m2e-durability-realtime.json \
  m2e-upgrade-integrity.json \
  m2e-summary.json \
  m2e-restart-summary.json; do
  test -s "$artifact_root/$artifact"
done

printf '%s\n' "M2E fixed work lifecycle, dependency DAG, atomicity, authority, realtime, and replay smoke passed"
