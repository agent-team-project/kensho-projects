#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

OUT_DIR="${GAUNTLET_OUT_DIR:-target/release-evidence/uci-gauntlet}"
GAMES="${GAUNTLET_GAMES:-2}"
ROUNDS="${GAUNTLET_ROUNDS:-1}"
TC="${GAUNTLET_TC:-1+0.1}"

mkdir -p "$OUT_DIR"

if ! command -v cutechess-cli >/dev/null 2>&1; then
    printf 'SKIP optional UCI gauntlet: cutechess-cli is not installed locally.\n'
    exit 0
fi

if [ -z "${CHESS_GAUNTLET_OPPONENTS:-}" ]; then
    printf 'SKIP optional UCI gauntlet: set CHESS_GAUNTLET_OPPONENTS to colon-delimited local UCI engine paths.\n'
    exit 0
fi

cargo build -p chess-engine

IFS=':' read -r -a opponents <<<"$CHESS_GAUNTLET_OPPONENTS"
for opponent in "${opponents[@]}"; do
    if [ ! -x "$opponent" ]; then
        printf 'ERROR optional UCI gauntlet opponent is not executable: %s\n' "$opponent" >&2
        exit 1
    fi

    name="$(basename "$opponent")"
    pgn="$OUT_DIR/kensho-vs-$name.pgn"
    summary="$OUT_DIR/kensho-vs-$name.txt"
    printf 'Running cutechess-cli gauntlet against %s\n' "$opponent"
    cutechess-cli \
        -engine cmd="$ROOT/target/debug/chess-engine" name="Kensho" \
        -engine cmd="$opponent" name="$name" \
        -each proto=uci tc="$TC" \
        -rounds "$ROUNDS" \
        -games "$GAMES" \
        -repeat \
        -pgnout "$pgn" \
        >"$summary" 2>&1
    printf 'gauntlet summary: %s\n' "$summary"
    printf 'gauntlet pgn: %s\n' "$pgn"
done
