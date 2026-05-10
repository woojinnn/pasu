#!/usr/bin/env bash
# Build crates/abi-resolver/data/sourcify.json — the curated, in-memory
# Sourcify bundle the resolver loads at startup.
#
# For each entry:
#   1. Try EIP-1967 / ZOS / UUPS implementation slots via RPC.
#   2. If non-zero, fetch ABI from Sourcify under the *implementation* address
#      (so proxy contracts surface their real function set).
#   3. Otherwise fetch ABI from the contract itself.
#
# Result: one JSON file, keyed by *proxy* address with the implementation's
# ABI inside.
#
# Run from the workspace root:
#   ./crates/abi-resolver/scripts/curate_bundle.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TMP_DIR="$(mktemp -d /tmp/curate.XXXXXX)"
trap 'rm -rf "$TMP_DIR"' EXIT

# chain_id:address:label
ENTRIES=(
  "1:0xE592427A0AEce92De3Edee1F18E0157C05861564:Uniswap_V3_SwapRouter"
  "1:0x68b3465833fb72A70ecDF485E0e4C7bD8665Fc45:Uniswap_V3_SwapRouter02"
  "1:0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D:Uniswap_V2_Router02"
  "1:0x66a9893cc07d91d95644aedd05d03f95e1dba8af:Uniswap_Universal_Router"
  "1:0x3fC91A3afd70395Cd496C647d5a6CC9D4B2b7FAD:Uniswap_Universal_Router_v1.2"
  "1:0x87870bca3f3fd6335c3f4ce8392d69350b4fa4e2:Aave_V3_Pool"
  "1:0xd01607c3C5eCABa394D8be377a08590149325722:Aave_V3_WETHGateway"
  "1:0xBBBBBbbBBb9CC5e90e3b3Af64bdAF62C37EEFFCb:Morpho_Blue"
  "1:0x6566194141eefa99af43bb5aa71460ca2dc90245:Morpho_Bundler"
  "1:0xdAC17F958D2ee523a2206206994597C13D831ec7:USDT"
  "1:0xA0b86991C6218b36c1d19D4a2e9Eb0cE3606eB48:USDC"
  "1:0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2:WETH"
  "1:0x6B175474E89094C44Da98b954EedeAC495271d0F:DAI"
  "1:0x000000000022D473030F116dDEE9F6B43aC78BA3:Permit2"
  "1:0xc3d688B66703497DAA19211EEdff47f25384cdc3:Compound_V3_USDC_Comet"
  "1:0x59E16fcCd424Cc24e280Be16E11Bcd56fb0CE547:ENS_ETHRegistrarController"
)

SLOTS=(
  "0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc"  # EIP-1967
  "0x7050c9e0f4ca769c69bd3a8ef740bc37934f8e2c036e5a723fd8ee048ed3f8c3"  # ZOS / FiatTokenProxy
  "0xc5f16f0fcc639fa48a6947836d9850f504798523bf8c9a3a87d5876cf622bcf7"  # ERC-1822 UUPS
)

rpc_for() {
  case "$1" in
    1) echo "https://ethereum-rpc.publicnode.com" ;;
    56) echo "https://bsc-dataseed1.binance.org" ;;
    *) echo ""; return 1 ;;
  esac
}

resolve_impl() {
  local chain="$1" addr="$2"
  local rpc; rpc=$(rpc_for "$chain")
  for slot in "${SLOTS[@]}"; do
    local result
    result=$(curl -sX POST "$rpc" -H "Content-Type: application/json" \
      -d "{\"jsonrpc\":\"2.0\",\"method\":\"eth_getStorageAt\",\"params\":[\"$addr\",\"$slot\",\"latest\"],\"id\":1}" 2>/dev/null \
      | jq -r .result 2>/dev/null)
    [ -z "$result" ] && continue
    local impl="0x${result:26}"
    if [ "$impl" != "0x0000000000000000000000000000000000000000" ]; then
      echo "$impl"
      return
    fi
  done
  echo ""
}

mkdir -p "$TMP_DIR/entries"

for entry in "${ENTRIES[@]}"; do
  chain=$(echo "$entry" | cut -d: -f1)
  proxy_addr=$(echo "$entry" | cut -d: -f2)
  label=$(echo "$entry" | cut -d: -f3)

  impl=$(resolve_impl "$chain" "$proxy_addr")
  if [ -n "$impl" ]; then
    abi_source="$impl"
    note="proxy → $impl"
  else
    abi_source="$proxy_addr"
    note="self"
  fi

  abi_json=$(curl -sL --max-time 15 "https://sourcify.dev/server/files/any/$chain/$abi_source" \
    | jq -r '.files[] | select(.name == "metadata.json") | .content' 2>/dev/null \
    | jq '.output.abi | map(select(.type == "function"))' 2>/dev/null)

  if [ -z "$abi_json" ] || [ "$abi_json" = "null" ]; then
    echo "MISS  $label $proxy_addr ($note)"
    continue
  fi

  fn_count=$(echo "$abi_json" | jq 'length')
  jq -n --argjson chain "$chain" --arg addr "$proxy_addr" --argjson abi "$abi_json" \
    '{chain_id: $chain, address: $addr, abi: $abi}' > "$TMP_DIR/entries/${label}.json"
  echo "HIT   $label $proxy_addr ($fn_count fns, $note)"
done

OUT="$ROOT/data/sourcify.json"
mkdir -p "$(dirname "$OUT")"
jq -s '{contracts: .}' "$TMP_DIR"/entries/*.json > "$OUT"

echo "---"
echo "wrote: $OUT"
echo "  contracts: $(jq '.contracts | length' "$OUT")"
echo "  total functions: $(jq '[.contracts[].abi | length] | add' "$OUT")"
