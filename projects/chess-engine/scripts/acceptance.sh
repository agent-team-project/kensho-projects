#!/usr/bin/env bash
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT" || exit 1

OUT_DIR="${ACCEPTANCE_OUT_DIR:-target/release-evidence}"
LOG="$OUT_DIR/acceptance.log"
BASELINE_GAMES="${BASELINE_GAMES:-6}"
BASELINE_MAX_PLIES="${BASELINE_MAX_PLIES:-80}"
BASELINE_ENGINE_DEPTH="${BASELINE_ENGINE_DEPTH:-2}"

mkdir -p "$OUT_DIR"
: >"$LOG"

failures=0
pending=0

log() {
    printf '%s\n' "$*" | tee -a "$LOG"
}

run_required() {
    local label="$1"
    shift
    log ""
    log "== REQUIRED: $label"
    log "$ $*"
    if "$@" 2>&1 | tee -a "$LOG"; then
        log "PASS: $label"
    else
        local status=$?
        failures=$((failures + 1))
        log "FAIL: $label (exit $status)"
    fi
}

mark_pending() {
    local label="$1"
    local reason="$2"
    pending=$((pending + 1))
    log ""
    log "== PENDING: $label"
    log "$reason"
}

run_required "format" cargo fmt --check
run_required "clippy" cargo clippy --workspace --all-targets -- -D warnings
run_required "workspace tests" cargo test --workspace
run_required "uci smoke" cargo run -p chess-engine -- uci-smoke tests/fixtures/uci-smoke.txt
run_required "perft suite through depth 4" cargo run -p chess-engine -- perft-suite tests/fixtures/perft.epd --max-depth 4
run_required "deterministic baseline tournament smoke" cargo run -p chess-engine -- baseline-tournament --games "$BASELINE_GAMES" --max-plies "$BASELINE_MAX_PLIES" --engine-depth "$BASELINE_ENGINE_DEPTH" --out-dir "$OUT_DIR/baseline-tournament"
run_required "gui subprocess smoke" cargo run -p local-board -- --smoke --engine target/debug/chess-engine
run_required "optional local UCI gauntlet availability" scripts/uci-gauntlet.sh

tactical_positions=0
if [ -d tests/fixtures/tactics ]; then
    tactical_positions="$(
        find tests/fixtures/tactics -type f -name '*.epd' -exec awk 'NF && $1 !~ /^#/ { count++ } END { print count + 0 }' {} + |
            awk '{ total += $1 } END { print total + 0 }'
    )"
fi

if [ "$tactical_positions" -lt 300 ]; then
    mark_pending "full tactical threshold" "Found $tactical_positions tactical positions; SPEC.md requires at least 300 positions and >=85% at 1000 ms before claiming the release tactical gate."
else
    run_required "full tactical threshold" cargo run -p chess-engine -- tactics tests/fixtures/tactics/*.epd --movetime 1000
fi

mark_pending "full baseline strength floor" "Default acceptance runs a short deterministic smoke tournament. Run BASELINE_GAMES=200 BASELINE_MAX_PLIES=120 BASELINE_ENGINE_DEPTH=3 scripts/acceptance.sh before claiming the SPEC.md 95% baseline strength floor."
mark_pending "full mandatory perft depths" "Default acceptance uses --max-depth 4 for local turnaround. Run cargo run -p chess-engine -- perft-suite tests/fixtures/perft.epd for all mandatory depths before a final release claim."

log ""
log "== SUMMARY"
log "required_failures: $failures"
log "pending_or_future_gates: $pending"
log "evidence_log: $LOG"

if [ "$failures" -eq 0 ]; then
    exit 0
fi
exit 1
