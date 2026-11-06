#!/usr/bin/env bash
# Time a release launch against the real profile: where does the wait for the
# sidebar (bootstrap_profile) and the PR inbox go?
#
#   scripts/startup-timing.sh [runs]     # default 1 run
#   SKIP_BUILD=1 scripts/startup-timing.sh 3
#   APP_ARGS="--codex-home $HOME/.codex-personal" scripts/startup-timing.sh
#
# Builds `tauri build --no-bundle` when the sources are newer than the binary,
# launches it with PINGEX_TRACE_STARTUP=1 PINGEX_TRACE_IO=1, waits for
# `bootstrap_profile done` and then for the inbox to go quiet (no `[startup]`
# or `[io]` line for QUIET seconds), quits the app, and prints a summary.
# Full logs land in $OUT_DIR (default: a temp dir).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="$ROOT/src-tauri/target/release/pingex-app"
RUNS="${1:-1}"
QUIET="${QUIET:-5}"
TIMEOUT="${TIMEOUT:-120}"
OUT_DIR="${OUT_DIR:-$(mktemp -d -t pingex-startup)}"
mkdir -p "$OUT_DIR"

already_running() {
  if pgrep -x pingex-app >/dev/null; then
    echo "Pingex is already running; quit it first (single-instance would swallow this launch)." >&2
    exit 1
  fi
}

already_running

if [[ -z "${SKIP_BUILD:-}" ]]; then
  if [[ ! -x "$BIN" ]] || [[ -n "$(find "$ROOT/src" "$ROOT/src-tauri/src" "$ROOT/src-tauri/Cargo.toml" -newer "$BIN" -type f -print -quit)" ]]; then
    echo "building release binary…" >&2
    (cd "$ROOT" && bun run tauri build --no-bundle >"$OUT_DIR/build.log" 2>&1) || {
      tail -40 "$OUT_DIR/build.log" >&2
      exit 1
    }
  fi
fi

now_ms() { python3 -c 'import time; print(int(time.time()*1000))'; }

for run in $(seq 1 "$RUNS"); do
  # Checked again per run: the app may have been opened during the build.
  already_running
  log="$OUT_DIR/run-$run.log"
  : >"$log"
  # shellcheck disable=SC2086
  PINGEX_TRACE_STARTUP=1 PINGEX_TRACE_IO=1 "$BIN" ${APP_ARGS:-} >/dev/null 2>"$log" &
  pid=$!
  start=$(now_ms)
  booted=""
  last_size=0
  last_change=$start
  while :; do
    sleep 0.25
    t=$(now_ms)
    if ! kill -0 "$pid" 2>/dev/null; then
      echo "run $run: app exited early" >&2
      break
    fi
    size=$(wc -c <"$log")
    if [[ "$size" != "$last_size" ]]; then
      last_size=$size
      last_change=$t
    fi
    [[ -z "$booted" ]] && grep -q '\[startup\] bootstrap_profile done' "$log" && booted=1
    if [[ -n "$booted" ]] && (( t - last_change > QUIET * 1000 )); then
      break
    fi
    if (( t - start > TIMEOUT * 1000 )); then
      echo "run $run: timed out after ${TIMEOUT}s" >&2
      break
    fi
  done
  kill "$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true

  echo "=== run $run ($log)"
  python3 - "$log" <<'PY'
import re, sys, collections
lines = open(sys.argv[1], errors="replace").read().splitlines()
phase = re.compile(r"^\[startup\] (.+?) (\d+)ms \(\+(\d+)ms\)$")
mark = re.compile(r"^\[startup\] (.+?) \(\+(\d+)ms\)$")
io = re.compile(r"^\[io\] (\S+) (.*?) (\d+)ms")
phases, marks, io_total, io_count, last_review = [], {}, collections.Counter(), collections.Counter(), 0
for line in lines:
    if m := phase.match(line):
        name, ms, at = m.group(1), int(m.group(2)), int(m.group(3))
        phases.append((ms, at, name))
        if name.startswith("review_"):
            last_review = max(last_review, at)
    elif m := mark.match(line):
        marks.setdefault(m.group(1), int(m.group(2)))
    elif m := io.match(line):
        io_total[m.group(1)] += int(m.group(3)); io_count[m.group(1)] += 1
    elif line.startswith("[startup]"):
        print("  " + line)
print(f"  launch -> setup done:              {marks.get('setup done', '?')}ms")
print(f"  launch -> bootstrap_profile done:  {marks.get('bootstrap_profile done', '?')}ms")
print(f"  launch -> last inbox command done: {last_review or '?'}ms")
print("  slowest phases (ms, ended at +ms):")
for ms, at, name in sorted(phases, reverse=True)[:25]:
    print(f"    {ms:>7}  +{at:<7} {name}")
for program in io_total:
    print(f"  [io] {program}: {io_count[program]} calls, {io_total[program]}ms total (serial sum)")
PY
done
