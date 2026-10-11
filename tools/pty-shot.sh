#!/usr/bin/env bash
# tools/pty-shot.sh — repeatable terminal capture of the graph view (W1 / lane b1).
#
# Usage:   tools/pty-shot.sh [name]
# Env:     ROWS (default 24)  COLS (default 80)  WAIT (seconds before 'q', default 1)
#          MOCK=0 captures the current directory instead of the mock fixture
#          KEYS   comma-separated keys/actions sent before 'q' (W4 nav evidence),
#                 e.g. KEYS=j,j,down,right,enter,s
#                 named keys: up down right left enter pgup pgdn home end esc
#                 anything else is sent as a literal character
#          ARGS   extra flags passed to the binary (e.g. ARGS=--text-list). When set,
#                 the deterministic .frame step is skipped (the mode is interactive).
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
ARGS="${ARGS:-}"
KEYS="${KEYS:-}"

OUT="$ROOT/target/pty"
mkdir -p "$OUT"

cargo build --quiet --manifest-path "$ROOT/Cargo.toml"
BIN="$ROOT/target/debug/git-graph-terminal"

CMD="stty rows $ROWS cols $COLS; exec '$BIN'"
if [[ "$MOCK" == "1" ]]; then
  CMD="$CMD --mock"
fi
if [[ -n "$ARGS" ]]; then
  CMD="$CMD $ARGS"
fi

echo "== pty capture: ${COLS}x${ROWS} (name=$NAME, mock=$MOCK, keys=${KEYS:-<none>}) =="

# Emit one key at a time with a small gap so the TUI redraws between presses.
# Named keys become the crossterm escape sequences the app already handles.
send_keys() {
  local spec="$1"
  local IFS=','
  for token in $spec; do
    case "$token" in
      up)    printf '\033[A' ;;
      down)  printf '\033[B' ;;
      right) printf '\033[C' ;;
      left)  printf '\033[D' ;;
      enter|cr) printf '\r' ;;
      pgup|pageup)     printf '\033[5~' ;;
      pgdn|pagedown)   printf '\033[6~' ;;
      home)  printf '\033[H' ;;
      end)   printf '\033[F' ;;
      esc)   printf '\033' ;;
      *)     printf '%s' "$token" ;;
    esac
    sleep 0.15
  done
}

# A pty at the requested size, then the scripted keys (if any) and 'q' after WAIT
# seconds so the app exits cleanly.
if [[ -n "$KEYS" ]]; then
  ( sleep "$WAIT"; send_keys "$KEYS"; sleep 0.3; printf 'q' ) \
    | timeout 15s script -q -c "$CMD" "$OUT/$NAME.raw" > /dev/null
else
  ( sleep "$WAIT"; printf 'q' ) \
    | timeout 10s script -q -c "$CMD" "$OUT/$NAME.raw" > /dev/null
fi

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

# Deterministic frame (no pty) so CI can assert the shot exactly. Skipped when ARGS
# selects an interactive mode (`--text-list`), where `.txt` is the artifact.
if [[ -n "$ARGS" ]]; then
  echo "frame   -> (skipped: ARGS=$ARGS is interactive)"
elif [[ "$MOCK" == "1" ]]; then
  "$BIN" --dump --mock > "$OUT/$NAME.frame"
  echo "frame   -> target/pty/$NAME.frame"
else
  "$BIN" --dump > "$OUT/$NAME.frame"
  echo "frame   -> target/pty/$NAME.frame"
fi

echo "raw     -> target/pty/$NAME.raw"
echo "shot    -> target/pty/$NAME.txt"
echo "strings -> target/pty/$NAME.strings.txt"
