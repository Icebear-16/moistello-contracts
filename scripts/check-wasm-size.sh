#!/usr/bin/env bash
# check-wasm-size.sh — Enforce per-contract WASM binary size budgets.
#
# Usage:
#   ./scripts/check-wasm-size.sh [WASM_DIR]
#
# Arguments:
#   WASM_DIR   Directory containing compiled WASM files.
#              Defaults to: target/wasm32v1-none/release
#
# Exit codes:
#   0  All contracts are within budget.
#   1  One or more contracts exceed their budget.

set -euo pipefail

WASM_DIR="${1:-target/wasm32v1-none/release}"

# ─── Budget table (bytes) ────────────────────────────────────────────────────
# Format: "wasm_filename:budget_bytes:label"
# 64 KB = 65536, 32 KB = 32768
BUDGETS=(
  "circle.wasm:65536:circle (64 KB)"
  "circle_factory.wasm:32768:circle-factory (32 KB)"
  "treasury.wasm:32768:treasury (32 KB)"
  "reputation_registry.wasm:32768:reputation-registry (32 KB)"
  "governance.wasm:65536:governance (64 KB)"
)

# ─── Colours ─────────────────────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BOLD='\033[1m'
RESET='\033[0m'

PASS=0
FAIL=0
SKIP=0

# ─── Header ──────────────────────────────────────────────────────────────────
echo ""
echo -e "${BOLD}WASM Size Budget Check${RESET}"
echo -e "${BOLD}WASM directory: ${RESET}${WASM_DIR}"
echo ""
printf "${BOLD}%-35s %10s %10s %8s %s${RESET}\n" "Contract" "Size" "Budget" "Delta" "Status"
printf '%s\n' "$(printf '─%.0s' {1..75})"

# ─── Check each contract ─────────────────────────────────────────────────────
for entry in "${BUDGETS[@]}"; do
  IFS=':' read -r filename budget label <<< "$entry"
  filepath="${WASM_DIR}/${filename}"

  if [[ ! -f "$filepath" ]]; then
    printf "%-35s %10s %10s %8s " "$label" "—" "$(numfmt --to=iec-i --suffix=B "$budget" 2>/dev/null || echo "${budget}B")" "—"
    echo -e "${YELLOW}SKIP (not built)${RESET}"
    (( SKIP++ )) || true
    continue
  fi

  size=$(wc -c < "$filepath")
  delta=$(( size - budget ))

  # Human-readable sizes
  size_hr=$(numfmt --to=iec-i --suffix=B "$size"   2>/dev/null || echo "${size}B")
  budget_hr=$(numfmt --to=iec-i --suffix=B "$budget" 2>/dev/null || echo "${budget}B")

  if [[ "$delta" -le 0 ]]; then
    delta_hr="-$(numfmt --to=iec-i --suffix=B "$(( -delta ))" 2>/dev/null || echo "$(( -delta ))B")"
    printf "%-35s %10s %10s %8s " "$label" "$size_hr" "$budget_hr" "$delta_hr"
    echo -e "${GREEN}PASS${RESET}"
    (( PASS++ )) || true
  else
    delta_hr="+$(numfmt --to=iec-i --suffix=B "$delta" 2>/dev/null || echo "${delta}B")"
    printf "%-35s %10s %10s %8s " "$label" "$size_hr" "$budget_hr" "$delta_hr"
    echo -e "${RED}FAIL — exceeds budget by ${delta_hr}${RESET}"
    (( FAIL++ )) || true
  fi
done

# ─── Summary ─────────────────────────────────────────────────────────────────
printf '%s\n' "$(printf '─%.0s' {1..75})"
echo ""
echo -e "Results: ${GREEN}${PASS} passed${RESET}  ${RED}${FAIL} failed${RESET}  ${YELLOW}${SKIP} skipped${RESET}"
echo ""

if [[ "$FAIL" -gt 0 ]]; then
  echo -e "${RED}${BOLD}WASM size budget check FAILED.${RESET}"
  echo "Reduce contract size by:"
  echo "  1. Running 'make optimize' (stellar contract optimize) to compress WASM"
  echo "  2. Removing unused dependencies from Cargo.toml"
  echo "  3. Using 'cargo bloat --release --target wasm32-unknown-unknown' to find large symbols"
  echo ""
  exit 1
fi

echo -e "${GREEN}${BOLD}All WASM size checks passed.${RESET}"
echo ""
exit 0
