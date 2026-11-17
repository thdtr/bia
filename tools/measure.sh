#!/usr/bin/env bash
# Measures BIA on this machine's GPU (wgpu) and CPU (tiny-skia) renderers:
# memory, CPU per key while scrolling and typing, CPU while idle, and the
# memory peak after a lot of terminal output.
#
# Nothing is typed for you: on Wayland a script can't safely send keys to
# one window. The steps that need the keyboard print a countdown here; you
# hold a key in BIA's window until STOP shows up.
#
# BIA runs with a throwaway config folder, so your settings, backups and
# open files are left alone.
#
# Usage: tools/measure.sh [binary...]
#   With no binaries: the installed `bia` and this repository's
#   target/release/bia (build it first with `cargo build --release -p bia`).
#   BACKENDS="wgpu" to skip tiny-skia.
set -euo pipefail

cd "$(dirname "$0")/.."
BACKENDS=${BACKENDS:-"wgpu tiny-skia"}
HOLD=10 # seconds a key is held
if [ $# -gt 0 ]; then
  BINARIES=("$@")
else
  BINARIES=("$(command -v bia || echo "$HOME/.local/bin/bia")" "$PWD/target/release/bia")
fi
for b in "${BINARIES[@]}"; do
  [ -x "$b" ] || { echo "Not found: $b (cargo build --release -p bia?)"; exit 1; }
done

OUT=$(mktemp -d "${TMPDIR:-/tmp}/bia-measure.XXXXXX")
FIX="$OUT/fixtures"
CSV="$OUT/results.csv"
TICK=$(getconf CLK_TCK)

# Key repeat, to turn "a key held for 10 s" into a number of keys.
interval=30 delay=500
if command -v gsettings >/dev/null; then
  interval=$(gsettings get org.gnome.desktop.peripherals.keyboard repeat-interval | awk '{print $2}')
  delay=$(gsettings get org.gnome.desktop.peripherals.keyboard delay | awk '{print $2}')
fi
KEYS=$(( (HOLD * 1000 - delay) / interval + 1 ))

echo "Preparing files in $FIX..."
mkdir -p "$FIX/repo"
awk 'BEGIN { print "fn main() {"; for (i = 0; i < 45000; i++) printf "    let value_%d = compute(%d, \"string %d\"); // comment %d\n", i, i, i, i; print "}" }' > "$FIX/repo/mid.rs"
for i in $(seq 3000); do : > "$FIX/repo/file_$i.txt"; done
git -C "$FIX/repo" init -q
git -C "$FIX/repo" add mid.rs
git -C "$FIX/repo" -c user.name=m -c user.email=m@m -c commit.gpgsign=false commit -qm init

echo "binary,version,backend,metric,value" > "$CSV"
cpu() { awk '{print $14 + $15}' "/proc/$1/stat"; }
kb() { awk -v k="$2:" '$1 == k {print $2}' "/proc/$1/$3"; }
mb() { awk -v k="$1" 'BEGIN { printf "%.1f", k / 1024 }'; }

# Waits for Enter, counts down, and samples CPU while a key is held.
# Prints CPU milliseconds per key; everything else goes to the terminal.
held() { # pid, key
  {
    read -rp "  Click into BIA's text, then press Enter here. "
    echo -n "  Hold $2 in BIA in "
    for n in 3 2 1; do echo -n "$n.. "; sleep 1; done
    echo "GO"
  } >&2
  local c1 c2
  c1=$(cpu "$1"); sleep "$HOLD"; c2=$(cpu "$1")
  echo "  STOP (let go)" >&2
  sleep 1
  awk -v t=$(( (c2 - c1) * 1000 / TICK )) -v k="$KEYS" 'BEGIN { printf "%.1f", t / k }'
}

record() { echo "$1,$2,$3,$4,$5" >> "$CSV"; printf "  %-34s %s\n" "$4" "$5"; }

for bin in "${BINARIES[@]}"; do
  version=$("$bin" --version 2>/dev/null | awk '{print $2}')
  for backend in $BACKENDS; do
    label="$(basename "$(dirname "$bin")")/$(basename "$bin") $version, $backend"
    echo
    echo "=== $label ==="
    cfg="$OUT/config-$RANDOM"
    mkdir -p "$cfg"
    ICED_BACKEND=$backend BIA_CONFIG_DIR=$cfg "$bin" "$FIX/repo" "$FIX/repo/mid.rs:20000" \
      >"$OUT/bia.log" 2>&1 &
    pid=$!
    sleep 5
    if ! kill -0 "$pid" 2>/dev/null; then
      echo "  BIA didn't start with $backend:"; tail -5 "$OUT/bia.log"; continue
    fi
    r() { record "$bin" "$version" "$backend" "$1" "$2"; }

    r "open: RSS (MB)" "$(mb "$(kb "$pid" VmRSS status)")"
    r "open: PSS (MB)" "$(mb "$(kb "$pid" Pss smaps_rollup)")"

    echo "  Scrolling: hold Page Down."
    r "scroll: CPU per key (ms)" "$(held "$pid" "Page Down")"
    r "scroll: RSS after (MB)" "$(mb "$(kb "$pid" VmRSS status)")"

    echo "  Typing: hold the x key (in the middle of the file)."
    r "typing: CPU per key (ms)" "$(held "$pid" "x")"

    read -rp "  Leave BIA focused and don't touch anything for 20 s. Press Enter to start. "
    sleep 10
    c1=$(cpu "$pid"); sleep 10; c2=$(cpu "$pid")
    r "idle (focused): CPU in 10 s (ms)" "$(( (c2 - c1) * 1000 / TICK ))"

    echo "  Terminal: press Ctrl+\` in BIA and run:  seq 1 3000000"
    read -rp "  Press Enter here when it has finished. "
    r "terminal: memory peak (MB)" "$(mb "$(kb "$pid" VmHWM status)")"

    kill "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
  done
done

# Side by side: one column per binary and renderer.
echo
awk -F, 'NR > 1 {
  col = $1 " " $3
  if (!(col in seen)) { seen[col] = 1; cols[++nc] = col }
  if (!($4 in row)) { row[$4] = 1; rows[++nr] = $4 }
  v[$4, col] = $5
}
END {
  for (c = 1; c <= nc; c++) printf "  [%d] %s\n", c, cols[c]
  printf "\n  %-34s", ""; for (c = 1; c <= nc; c++) printf "%12s", "[" c "]"; print ""
  for (r = 1; r <= nr; r++) {
    printf "  %-34s", rows[r]
    for (c = 1; c <= nc; c++) printf "%12s", ((rows[r], cols[c]) in v ? v[rows[r], cols[c]] : "-")
    print ""
  }
}' "$CSV"

echo
echo "Held keys count as $KEYS keys each (${HOLD} s, repeat every ${interval} ms after ${delay} ms)."
echo "GPU: $(lspci 2>/dev/null | grep -iE 'vga|3d' | sed 's/.*: //' | head -1)"
echo "Results: $CSV"
