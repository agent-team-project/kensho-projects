#!/usr/bin/env bash
set -euo pipefail

# Force Kensho-launched agents through the local Codex CLI ChatGPT login path.
# Do not let Platform/API-key environment variables select usage-based auth.
unset OPENAI_API_KEY
unset CODEX_API_KEY
unset CODEX_ACCESS_TOKEN
unset OPENAI_BASE_URL
unset OPENAI_ORG_ID
unset OPENAI_PROJECT

REAL_CODEX_BIN="${CODEX_REAL_BIN:-$(command -v codex || true)}"
if [[ -z "$REAL_CODEX_BIN" || ! -x "$REAL_CODEX_BIN" ]]; then
  printf 'codex executable not found; set CODEX_REAL_BIN or add codex to PATH\n' >&2
  exit 127
fi

exec "$REAL_CODEX_BIN" -c 'forced_login_method="chatgpt"' "$@"
