// Phase 5D — mock `scopeball.evaluate_v3` method for the policy-rpc daemon.
//
// The Phase 5 cutover establishes the SW ↔ rpc-server JSON-RPC 2.0 wire
// (`scopeball.evaluate_v3`) that hands the rpc-server a typed
// (Action[] + EvalContext + WalletId) tuple and returns a fully
// post-processed `policyRequest` (typed actions + state_before/deltas/
// state_after) which the SW then forwards to the WASM
// `evaluate_policy_request_json` entry for Cedar evaluation.
//
// The real rpc-server implementation lives in `crates/simulation/sync/`
// + `crates/simulation/db/` (state persistence, reducer dispatch, delta
// merge). Those are Phase 6's responsibility. To unblock the SW round-
// trip today this method is an echo:
//
//   - `policyRequest.actions`      = caller-supplied `envelopes`
//   - `policyRequest.state_before` = {}
//   - `policyRequest.deltas`       = []
//   - `policyRequest.state_after`  = {}
//   - `diagnostics`                = []
//
// Phase 6 swaps the body for the real reducer + state sync without
// touching the SW client or the method's wire shape.

import {
  RpcMethodError,
  type JsonObject,
  type JsonValue,
} from "../types.js";
import { isRecord } from "../validation.js";
import type { MethodCatalogEntry } from "./catalog.js";

/**
 * Phase 5D catalog entry. The `String` typed params here are
 * placeholder — none of the Cedar aliases in
 * `policy_engine::schema::aliases` describes `Action` / `EvalContext` /
 * `WalletId` yet. Phase 6 will refine these once the FSM type system
 * lands in the schema (tsify export).
 *
 * The dashboard's manifest editor does NOT (today) consume this method
 * — `scopeball.evaluate_v3` is invoked by the SW orchestrator, not by
 * user-authored manifests. The catalog entry is here so the listing
 * `GET /v1/methods` returns it for parity with the rest of the bundled
 * set.
 */
export const scopeballEvaluateV3Catalog: MethodCatalogEntry = {
  name: "scopeball.evaluate_v3",
  description:
    "Phase 5D mock — echo (envelopes, eval_context, wallet_id) back as policyRequest.actions with empty state_before / deltas / state_after. Real implementation arrives in Phase 6 (reducer + state sync).",
  params: {
    wallet_id: {
      type: "String",
      required: true,
      description:
        "WalletId (address + chains). Carried verbatim by the mock; Phase 6 will key state persistence off this.",
    },
    envelopes: {
      type: "String",
      required: true,
      description:
        "List of caller-built Action envelopes (typed PDF FSM ActionBody). The mock echoes these into policyRequest.actions.",
    },
    eval_context: {
      type: "String",
      required: true,
      description:
        "EvalContext (chain + clock + RequestKind + SimulationMode + envelope_index).",
    },
  },
  returns: { kind: "record", type: "UsdValuation" },
  origin: "bundled",
};

/**
 * Parsed `scopeball.evaluate_v3` params. The Phase 5D mock keeps the
 * field types loose (`unknown`/`JsonValue`) because the actual
 * `Action` / `EvalContext` / `WalletId` shapes are defined by
 * `crates/simulation/{state,reducer}/` and re-exported to TS through
 * tsify. Binding them strictly here would couple the mock to a moving
 * Rust source-of-truth; Phase 6 swaps in the typed import once the
 * schema firms up.
 */
interface EvaluateV3Params {
  wallet_id: JsonValue;
  envelopes: JsonValue;
  eval_context: JsonValue;
}

function parseEvaluateV3Params(value: unknown): EvaluateV3Params {
  if (!isRecord(value)) {
    throw new RpcMethodError(
      "invalid_params",
      "scopeball.evaluate_v3 params must be an object",
    );
  }
  const { wallet_id, envelopes, eval_context } = value;
  if (wallet_id === undefined) {
    throw new RpcMethodError(
      "invalid_params",
      "scopeball.evaluate_v3.params.wallet_id is required",
    );
  }
  if (!Array.isArray(envelopes)) {
    throw new RpcMethodError(
      "invalid_params",
      "scopeball.evaluate_v3.params.envelopes must be an array",
    );
  }
  if (eval_context === undefined) {
    throw new RpcMethodError(
      "invalid_params",
      "scopeball.evaluate_v3.params.eval_context is required",
    );
  }
  return {
    wallet_id: wallet_id as JsonValue,
    envelopes: envelopes as JsonValue,
    eval_context: eval_context as JsonValue,
  };
}

/**
 * Factory: produce the `scopeball.evaluate_v3` echo handler. Mirrors
 * the `create*Method` pattern the bundled mock-host-capabilities methods
 * use so `registry.ts` wires it consistently.
 */
export function createScopeballEvaluateV3Method(): (
  params: unknown,
) => Promise<JsonObject> {
  return async (params: unknown): Promise<JsonObject> => {
    const parsed = parseEvaluateV3Params(params);

    // Phase 5D echo: the wire body returned from the rpc-server slots
    // straight into `JsonRpcReply.result` on the SW side. The SW then
    // hands `policyRequest` to the WASM `evaluate_policy_request_json`
    // entry — which is itself a Phase 5B stub, so the round-trip is
    // observability-only at present.
    const policyRequest: JsonObject = {
      actions: parsed.envelopes,
      state_before: {},
      deltas: [],
      state_after: {},
      // Echo the wallet + eval_context back so Phase 6 can be wired
      // incrementally (the real implementation will key state lookup
      // off these and may drop them from the response body once
      // persistence works).
      wallet_id: parsed.wallet_id,
      eval_context: parsed.eval_context,
    };

    return {
      policyRequest,
      diagnostics: [],
    };
  };
}
