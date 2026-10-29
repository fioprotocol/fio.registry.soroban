#!/usr/bin/env bash
# Reproduces the FIO usage statistics quoted in the Stellar grant application.
# Counts on-chain actions on FIO mainnet via the Hyperion history API.
#
#   scripts/fio-stats.sh                      # last 12 months ending now (UTC)
#   AFTER=2025-10-29T00:00:00 BEFORE=2026-10-29T00:00:00 scripts/fio-stats.sh
#   HYPERION=https://fio.cryptolions.io scripts/fio-stats.sh
set -euo pipefail

HYPERION="${HYPERION:-https://fio.eosusa.io}"
BEFORE="${BEFORE:-$(date -u +%Y-%m-%dT%H:%M:%S)}"
if [ -z "${AFTER:-}" ]; then
  AFTER="$(date -u -v-12m +%Y-%m-%dT%H:%M:%S 2>/dev/null || date -u -d '12 months ago' +%Y-%m-%dT%H:%M:%S)"
fi

# count <action> [after] [before] -> total number of irreversible actions
count() {
  local url="$HYPERION/v2/history/get_actions?act.name=$1&limit=1&track=true"
  [ -n "${2:-}" ] && url="$url&after=$2"
  [ -n "${3:-}" ] && url="$url&before=$3"
  curl -fsS --retry 3 "$url" | jq -r '.total.value'
}

regaddress=$(count regaddress "$AFTER" "$BEFORE")
regdomadd=$(count regdomadd "$AFTER" "$BEFORE")
regdomain=$(count regdomain "$AFTER" "$BEFORE")
newfundsreq=$(count newfundsreq "$AFTER" "$BEFORE")
all_regaddress=$(count regaddress)
all_regdomadd=$(count regdomadd)

cat <<OUT
source:  $HYPERION (Hyperion, FIO mainnet)
window:  $AFTER .. $BEFORE UTC

Last 12 months
  X  new FIO Handles            $((regaddress + regdomadd))   (regaddress $regaddress + regdomadd $regdomadd)
     new FIO Domains            $regdomain
  Y  FIO Requests sent          $newfundsreq   (newfundsreq)
  Z  handle resolutions         n/a   (get_pub_address is a read-only API call, not an on-chain action)

All time
     FIO Handles registered     $((all_regaddress + all_regdomadd))   (regaddress $all_regaddress + regdomadd $all_regdomadd)

Live count: $HYPERION/v2/history/get_actions?act.name=regaddress&limit=1&track=true  (field total.value)
OUT
