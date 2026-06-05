/**
 * Built-in sample actions for on-demand denial diagnosis ("Simulate").
 *
 * Keyed by the Pascal action-uid id a policy targets (e.g. `"Swap"`, from
 * `policy.scope.action.entity.id`). Each entry returns a self-contained
 * `{ action, meta, tx, bundles, results }` the diagnosis runner feeds to the
 * WASM probe oracle.
 *
 * The `action` / `meta` JSON below is captured VERBATIM from the Rust
 * `swap_sample_with_slippage(150)` serializer (the exact serde shape the WASM
 * `run_diagnosis_probes_v2_json` lowering expects). Slippage is baked at 150 so
 * a shipped `forbid … when { context.slippageBp > 100 }` guard trips. To
 * regenerate: temporarily print `serde_json::to_string(&body)` / `&meta` inside
 * `crates/policy-engine-wasm/src/action_eval_exports.rs`'s
 * `shipped_swap_policy_fires_on_child_swap_position` test, run
 * `cargo test -p policy-engine-wasm shipped_swap_policy_fires_on_child_swap_position -- --nocapture`,
 * then remove the print.
 */

import type { DiagnosisRequestDto } from "../server-api/diagnosis";

/** A sample diagnosis request minus `probes` — the caller wraps the policy's
 *  own probes onto it at Simulate time (`{ ...SAMPLE_ACTIONS[id](), probes }`). */
export type SampleRequest = Omit<DiagnosisRequestDto, "probes">;

/**
 * Minimal, self-contained sample requests keyed by action-uid id (Pascal).
 *
 * To add a sample for another action type: add an entry keyed by its Pascal
 * action id (the `policy.scope.action.entity.id` a policy targets, e.g.
 * `"Transfer"`), and fill `action`/`meta` from the matching Rust serializer
 * output (see the regeneration steps above). `tx` can stay as-is; keep `bundles`
 * empty and `results` `{}` for a base-context sim. Surfaces look this map up by
 * the policy's action id and show "이 액션의 샘플이 없습니다" when it is missing.
 */
export const SAMPLE_ACTIONS: Record<string, () => SampleRequest> = {
  Swap: () => ({
    action: {
      domain: "amm",
      action: "swap",
      venue: {
        name: "uniswap_v3",
        chain: "eip155:42161",
        pool: "0xc6962004f452be9203591991d15f6b388e09e8d0",
        fee_tier_bp: 500,
      },
      params: {
        token_in: {
          key: {
            standard: "erc20",
            chain: "eip155:42161",
            address: "0xaf88d065e77c8cc2239327c5edb3a432268e5831",
          },
        },
        token_out: {
          key: {
            standard: "erc20",
            chain: "eip155:42161",
            address: "0x82af49447d8a07e3bd95bd0d56f35241523fbab1",
          },
        },
        direction: {
          kind: "exact_input",
          amount_in: "0x3b9aca00",
          min_amount_out: "0x429d069189e0000",
        },
        recipient: "0x000000000000000000000000000000000000a01c",
        slippage_bp: 150,
      },
      live_inputs: {
        route: {
          value: {
            paths: [
              {
                share_bp: 10000,
                hops: [
                  {
                    token_in: {
                      key: {
                        standard: "erc20",
                        chain: "eip155:42161",
                        address: "0xaf88d065e77c8cc2239327c5edb3a432268e5831",
                      },
                    },
                    token_out: {
                      key: {
                        standard: "erc20",
                        chain: "eip155:42161",
                        address: "0x82af49447d8a07e3bd95bd0d56f35241523fbab1",
                      },
                    },
                    venue: {
                      name: "uniswap_v3",
                      chain: "eip155:42161",
                      pool: "0xc6962004f452be9203591991d15f6b388e09e8d0",
                      fee_tier_bp: 500,
                    },
                    pool_state: {
                      kind: "concentrated",
                      sqrt_price_x96: "0x1",
                      tick: 0,
                      liquidity: "0x0",
                      ticks: [],
                    },
                    effective_fee_bp: 5,
                    estimated_out: "0x43b93e2507e8000",
                  },
                ],
                estimated_out: "0x43b93e2507e8000",
              },
            ],
          },
          source: {
            kind: "onchain_view",
            chain: "eip155:42161",
            contract: "0xc6962004f452be9203591991d15f6b388e09e8d0",
            function: "slot0()",
            decoder_id: "uniswap_v3_slot0",
          },
          synced_at: 1738000000,
          ttl: 12,
        },
        expected_amount_out: {
          value: "0x43b93e2507e8000",
          source: {
            kind: "onchain_view",
            chain: "eip155:42161",
            contract: "0xc6962004f452be9203591991d15f6b388e09e8d0",
            function: "slot0()",
            decoder_id: "uniswap_v3_slot0",
          },
          synced_at: 1738000000,
        },
        price_impact_bp: {
          value: 12,
          source: {
            kind: "onchain_view",
            chain: "eip155:42161",
            contract: "0xc6962004f452be9203591991d15f6b388e09e8d0",
            function: "slot0()",
            decoder_id: "uniswap_v3_slot0",
          },
          synced_at: 1738000000,
        },
        gas_estimate: {
          value: "0x2bf20",
          source: {
            kind: "oracle_feed",
            provider: "pyth",
            feed_id: "gas/arbitrum",
          },
          synced_at: 1738000000,
        },
      },
    },
    meta: {
      submitted_at: 1738000000,
      submitter: "0x000000000000000000000000000000000000a01c",
      nature: {
        kind: "onchain_tx",
        chain: "eip155:42161",
        nonce: 42,
        gas_limit: "0x30d40",
        gas_price: {
          value: "0x5f5e100",
          source: {
            kind: "oracle_feed",
            provider: "pyth",
            feed_id: "ETH/USD",
          },
          synced_at: 1738000000,
        },
        value: "0x0",
      },
    },
    tx: {
      chain_id: "eip155:42161",
      from: "0x1111111111111111111111111111111111111111",
      to: "0x2222222222222222222222222222222222222222",
    },
    bundles: [],
    results: {},
  }),
};
