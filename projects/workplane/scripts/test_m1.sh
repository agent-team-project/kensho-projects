#!/bin/sh
set -eu

cd "$(dirname "$0")/.."
artifact_root="${WORKPLANE_M1_EVIDENCE_DIR:-$PWD/target/agent-evidence/m1}"
m2_artifact_root="${WORKPLANE_M2_EVIDENCE_DIR:-$PWD/target/agent-evidence/m2}"
client_proxy_pid=""
client_proxy_log="${TMPDIR:-/tmp}/workplane-client-api-proxy-$$.log"

cleanup() {
  if [ -n "$client_proxy_pid" ]; then kill "$client_proxy_pid" >/dev/null 2>&1 || true; fi
  rm -f "$client_proxy_log"
  docker compose down --volumes --remove-orphans >/dev/null 2>&1 || true
}
trap cleanup EXIT INT TERM

docker compose down --volumes --remove-orphans >/dev/null 2>&1 || true
rm -rf "$artifact_root"
rm -rf "$m2_artifact_root"
mkdir -p "$artifact_root/api" "$artifact_root/browser"
mkdir -p "$m2_artifact_root"
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

docker build --network none --target smoke -t workplane-smoke:m2 . >/dev/null
if ! docker run --rm --network workplane_default \
  -e WORKPLANE_API_BASE=http://api:8080 \
  -e WORKPLANE_DATABASE_URL='postgres://workplane:workplane-local-only@postgres:5432/workplane?sslmode=disable' \
  -e WORKPLANE_APP_DATABASE_URL='postgres://workplane_app:workplane-app-local-only@postgres:5432/workplane?sslmode=disable' \
  -v "$artifact_root/api:/evidence" workplane-smoke:m2; then
  docker compose logs --no-color >&2 || true
  exit 1
fi

cp "$artifact_root/migration.log" "$m2_artifact_root/migration.log"
for artifact in "$artifact_root"/api/m2-*.json; do
  test -f "$artifact"
  cp "$artifact" "$m2_artifact_root/"
done

api_ip="$(docker inspect -f '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' "$(docker compose ps -q api)")"
client_base="http://127.0.0.1:18081"
WORKPLANE_PROXY_TARGET="http://${api_ip}:8080" WORKPLANE_PROXY_ADDR="${client_base#http://}" \
  python3 scripts/http_proxy.py >"$client_proxy_log" 2>&1 &
client_proxy_pid=$!
i=0
until curl -fsS "$client_base/readyz" >/dev/null 2>&1; do
  i=$((i + 1))
  test "$i" -lt 30
  sleep 1
done
client_boundary_raw="$artifact_root/.generated-client-api-boundaries.raw"
if ! env WORKPLANE_LIVE_API_URL="$client_base" \
  npm --prefix web test -- --run src/api/client.live.test.ts \
  >"$client_boundary_raw" 2>&1; then
  sed "s|$PWD|<workplane>|g" "$client_boundary_raw" >&2
  rm -f "$client_boundary_raw"
  exit 1
fi
sed "s|$PWD|<workplane>|g" "$client_boundary_raw" >"$artifact_root/generated-client-api-boundaries.txt"
rm -f "$client_boundary_raw"
cat "$artifact_root/generated-client-api-boundaries.txt"
kill "$client_proxy_pid" >/dev/null 2>&1 || true
client_proxy_pid=""
rm -f "$client_proxy_log"

docker compose exec -T postgres psql -At -F '|' -U workplane -d workplane -c \
  "SELECT sequence,event_id,event_type,aggregate_version,actor_kind,actor_id,COALESCE(principal_id::text,''),command_id,request_id FROM domain_events ORDER BY sequence" \
  >"$artifact_root/event-rows.txt"

if ! WORKPLANE_BROWSER_EVIDENCE="$artifact_root/browser" scripts/test_browser.sh; then
  docker compose logs --no-color >&2 || true
  exit 1
fi
.venv/bin/python3 scripts/generate.py --check >"$artifact_root/generated-client-zero-diff.txt"

printf '%s\n' "M1 API/browser parity and M2A durable spine passed on offline PostgreSQL Compose"
