#!/bin/sh
set -eu

cd "$(dirname "$0")/.."
artifact_root="${WORKPLANE_M2C_EVIDENCE_DIR:-$PWD/target/agent-evidence/m2c}"

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

docker build --network none --target smoke -t workplane-smoke:m2c . >/dev/null
if ! docker run --rm --network workplane_default \
  --entrypoint /workplane-planning-smoke \
  -e WORKPLANE_API_BASE=http://api:8080 \
  -e WORKPLANE_DATABASE_URL='postgres://workplane:workplane-local-only@postgres:5432/workplane?sslmode=disable' \
  -e WORKPLANE_EVIDENCE_DIR=/evidence \
  -v "$artifact_root:/evidence" \
  workplane-smoke:m2c; then
  docker compose logs --no-color >&2 || true
  exit 1
fi

for artifact in \
  m2c-human-agent-parity.json \
  m2c-deliverables.json \
  m2c-forecasts.json \
  m2c-authority-denies.json \
  m2c-realtime-resume.json \
  m2c-durability.json \
  m2c-lifecycle.json \
  m2c-summary.json; do
  test -s "$artifact_root/$artifact"
done

printf '%s\n' "M2C lifecycle, deliverable, forecast, parity, durability, and authority smoke passed"
