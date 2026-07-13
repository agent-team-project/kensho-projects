#!/bin/sh
set -eu

cd "$(dirname "$0")/.."
browser_ip="${WORKPLANE_BROWSER_IP:-172.30.17.50}"
browser_base="${WORKPLANE_BROWSER_BASE:-http://127.0.0.1:18080}"
container="workplane-browser-api-${$}"
proxy_pid=""

cleanup() {
  if [ -n "$proxy_pid" ]; then kill "$proxy_pid" >/dev/null 2>&1 || true; fi
  docker rm -f "$container" >/dev/null 2>&1 || true
}
trap cleanup EXIT INT TERM

mkdir -p target/agent-evidence

docker run -d --name "$container" --network workplane_default --ip "$browser_ip" \
  --read-only --tmpfs /tmp:size=16m,mode=1777 \
  -e WORKPLANE_DATABASE_URL='postgres://workplane:workplane-local-only@postgres:5432/workplane?sslmode=disable' \
  -e WORKPLANE_PUBLIC_ORIGIN="$browser_base" \
  -e WORKPLANE_COOKIE_SECURE=false \
  -e WORKPLANE_TOKEN_HASH_KEY=local-only-key-material-32-bytes-minimum-change-me \
  -e WORKPLANE_BOOTSTRAP_ORG_ID=00000000-0000-4000-8000-000000000010 \
  -e WORKPLANE_BOOTSTRAP_HUMAN_ID=00000000-0000-4000-8000-000000000001 \
  -e WORKPLANE_BOOTSTRAP_HUMAN_EMAIL=human@workplane.local \
  -e WORKPLANE_BOOTSTRAP_HUMAN_PASSWORD=walking-slice-password \
  -e WORKPLANE_BOOTSTRAP_AGENT_ID=00000000-0000-4000-8000-000000000002 \
  -e WORKPLANE_BOOTSTRAP_AGENT_TOKEN=wpa_local_walking_slice_agent_token_00000000000000000001 \
  workplane-api >/dev/null

WORKPLANE_PROXY_TARGET="http://${browser_ip}:8080" WORKPLANE_PROXY_ADDR="${browser_base#http://}" \
  python3 scripts/http_proxy.py >target/agent-evidence/browser-proxy.log 2>&1 &
proxy_pid=$!

i=0
until curl -fsS "$browser_base/readyz" >/dev/null 2>&1; do
  i=$((i + 1))
  test "$i" -lt 30
  sleep 1
done

WORKPLANE_BROWSER_BASE="$browser_base" node scripts/browser_smoke.mjs
