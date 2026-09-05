#!/usr/bin/env bash
# Full local check, including byte-for-byte parity with the Python reference.
# Needs a Zelda 3 (USA) ROM, which cannot be committed or shipped to CI.
#
#   ./check.sh /path/to/zelda3.sfc [/path/to/translated.sfc ...]
#
# Any extra ROMs are translated releases. Each one is checked on its own
# (US + that language) and then all of them together, because the multi-
# language path is where an ordering bug would hide: the Python packs
# languages in the order given on the command line and produces a different
# file for `de,fr` than for `fr,de`. The module has no command line, so it
# sorts languages into the declaration order of kLanguages, and the oracle it
# is compared against is the Python run with that same order.
set -euo pipefail
cd "$(dirname "$0")"

ROM="${1:-}"
shift || true
LANG_ROMS=("$@")
cargo build --release --locked --target wasm32-unknown-unknown --lib
node test.mjs
node verify.mjs target/wasm32-unknown-unknown/release/zelda3_restool.wasm

if [[ -z "$ROM" ]]; then
  echo; echo "No ROM given - skipping output parity check."
  echo "Run './check.sh /path/to/zelda3.sfc' to compare against the Python reference."
  exit 0
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

# The Python is the oracle. It runs in two passes and writes a pile of
# intermediate files into whatever directory it is run from, so run it in a
# throwaway copy of tables/ rather than littering the working tree.
echo; echo "== Python reference =="
here="$PWD"
rom_abs="$(realpath "$ROM")"
cp -r "$here/.." "$tmp/tables"
rm -rf "$tmp/tables/wasm"
# The extract step reaches other/3x5_font.png through the sprite-sheet preview
# writer. None of it feeds the .dat, but it runs, so the oracle needs the
# directory beside tables/.
cp -r "$here/../../other" "$tmp/other"
( cd "$tmp/tables" && python3 restool.py --rom "$rom_abs" --extract-from-rom )
cp "$tmp/tables/zelda3_assets.dat" "$tmp/zelda3_assets.dat"

# Each translated ROM needs its own extract pass: --extract-dialogue writes
# dialogue_XX.txt and font_XX.png and exits, and the compile refuses to run
# without those files already on disk. The language code is auto-detected from
# the ROM header, so it is read back out of the file the pass just wrote.
codes=()
for lang_rom in ${LANG_ROMS+"${LANG_ROMS[@]}"}; do
  before="$(ls "$tmp/tables"/dialogue_*.txt 2>/dev/null || true)"
  ( cd "$tmp/tables" && python3 restool.py --rom "$(realpath "$lang_rom")" --extract-dialogue )
  after="$(ls "$tmp/tables"/dialogue_*.txt 2>/dev/null || true)"
  new_file="$(comm -13 <(echo "$before") <(echo "$after") | head -1)"
  code="$(basename "$new_file" .txt)"; code="${code#dialogue_}"
  codes+=("$code")
  cp "$lang_rom" "$tmp/rom_$code.sfc"
done

# The declaration order of kLanguages, which is the order the module packs in
# and therefore the order the oracle has to be built in.
canonical=(de fr fr-c en es pl pt redux nl sv)
sorted=()
for c in "${canonical[@]}"; do
  for have in ${codes+"${codes[@]}"}; do
    [[ "$have" == "$c" ]] && sorted+=("$c")
  done
done

echo "== wasm module =="
cargo build --release --locked --target wasm32-unknown-unknown --lib

# One parity check: build both sides for a language set and diff them.
# $1 is a label, the rest are language codes in canonical order.
check_build() {
  local label="$1"; shift
  local set=("$@")
  local dat="$tmp/oracle_$label.dat" out="$tmp/wasm_$label.dat"
  local roms=("$ROM")

  echo; echo "-- $label --"
  if [[ ${#set[@]} -eq 0 ]]; then
    cp "$tmp/zelda3_assets.dat" "$dat"
  else
    local joined; joined="$(IFS=,; echo "${set[*]}")"
    ( cd "$tmp/tables" && python3 restool.py --rom "$rom_abs" --languages "$joined" )
    cp "$tmp/tables/zelda3_assets.dat" "$dat"
    for c in "${set[@]}"; do roms+=("$tmp/rom_$c.sfc"); done
  fi

  # The module is handed the ROMs in an order that is deliberately not the
  # canonical one, so a run that happened to depend on registration order
  # would fail here rather than pass by luck.
  local shuffled=("${roms[0]}")
  local i
  for (( i=${#roms[@]}-1; i>=1; i-- )); do shuffled+=("${roms[i]}"); done

  node extract.mjs target/wasm32-unknown-unknown/release/zelda3_restool.wasm \
    "$out" "${shuffled[@]}"

  if cmp "$dat" "$out"; then
    echo "PASS - $label is byte-identical to the Python reference."
  else
    echo "FAIL - $label differs from the Python reference."
    # A whole-file cmp says nothing about which asset went wrong. compare.mjs
    # parses both directories and reports it per key.
    node compare.mjs "$dat" "$out" || true
    exit 1
  fi
}

check_build "US only"
for c in ${sorted+"${sorted[@]}"}; do
  check_build "US+$c" "$c"
done
if [[ ${#sorted[@]} -gt 1 ]]; then
  check_build "US+$(IFS=,; echo "${sorted[*]}")" "${sorted[@]}"
fi
cp "$tmp/wasm_US only.dat" "$tmp/wasm.dat"

# Record the hashes of this run so the published manifest can state what a
# correct extraction produces. Only written from a run that just passed parity,
# so the file cannot claim a hash the Python did not also produce.
node record-reference.mjs reference.json "$tmp/wasm.dat" "$ROM"

echo; echo "== ABI behaviour =="
node test-abi.mjs "$ROM" ${LANG_ROMS+"${LANG_ROMS[@]}"}
