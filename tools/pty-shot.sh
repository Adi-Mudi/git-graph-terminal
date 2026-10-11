#!/usr/bin/env bash
# tools/pty-shot.sh — repeatable terminal capture of the graph view (W1 / lane b1).
#
# Usage:   tools/pty-shot.sh [name]
# Env:     ROWS (default 24)  COLS (default 80)  WAIT (seconds before 'q', default 1)
#          MOCK=0 captures the current directory instead of the mock fixture
#
# Allocates a real pty at ROWSxCOLS, drives the TUI, captures it, and writes four
# artifacts under target/pty/ (git-ignored):
#
#   <name>.raw          raw pty bytes (alternate-screen escape sequences)
#   <name>.txt          readable shot: ANSI-stripped, one line per cursor move
#   <name>.strings.txt  `strings` view of the raw capture (stty + timeout + strings)
#   <name>.frame        deterministic `--dump [--mock]` frame (no pty; CI can diff it)
#
# M3's CI reuses this to attach a screenshot to each build.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NAME="${1:-mock-graph}"
ROWS="${ROWS:-24}"
COLS="${COLS:-80}"
WAIT="${WAIT:-1}"
MOCK="${MOCK:-1}"

OUT="$ROOT/target/pty"
mkdir -p "$OUT"

cargo build --quiet --manifest-path "$ROOT/Cargo.toml"
BIN="$ROOT/target/debug/git-graph-terminal"

CMD="stty rows $ROWS cols $COLS; exec '$BIN'"
if [[ "$MOCK" == "1" ]]; then
  CMD="$CMD --mock"
fi

echo "== pty capture: ${COLS}x${ROWS} (name=$NAME, mock=$MOCK) =="

# A pty at the requested size, then 'q' after WAIT seconds so the app exits cleanly.
( sleep "$WAIT"; printf 'q' ) \
  | timeout 10s script -q -c "$CMD" "$OUT/$NAME.raw" > /dev/null

# strings turns the escape-sequence stream into readable lines (stty + timeout + strings).
strings -n 2 "$OUT/$NAME.raw" > "$OUT/$NAME.strings.txt"

# Readable shot: turn each cursor move into a line break, then drop the escapes.
# The final frame in the file is the state the app left behind.
ESC=$(printf '\033')
sed -e "s/${ESC}\\[[0-9;]*H/\n/g" \
    -e "s/${ESC}\\[[0-9;?]*[a-zA-Z]//g" \
    -e "s/${ESC}[()][A-Za-z0-9]//g" \
    -e "s/${ESC}[=>]//g" \
    "$OUT/$NAME.raw" \
  | sed -e 's/[[:space:]]*$//' -e '/^$/d' > "$OUT/$NAME.txt"

# Deterministic frame (no pty) so CI can assert the shot exactly.
if [[ "$MOCK" == "1" ]]; then
  "$BIN" --dump --mock > "$OUT/$NAME.frame"
else
  "$BIN" --dump > "$OUT/$NAME.frame"
fi

echo "raw     -> target/pty/$NAME.raw"
echo "shot    -> target/pty/$NAME.txt"
echo "strings -> target/pty/$NAME.strings.txt"
echo "frame   -> target/pty/$NAME.frame"
