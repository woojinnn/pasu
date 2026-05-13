import Browser from "webextension-polyfill";
import init, * as wasmExports from "../wasm/policy_engine_wasm";
import {
  parseAction,
  parseTier1Plan,
  parseVerdict,
  parseWindowKeys,
  type ParsedAction,
  type Tier1Plan,
  type VerdictDto,
  type WindowKeys,
} from "./wasm-bridge.types";

export { WasmDecodeError } from "./wasm-bridge.types";
export type {
  ParsedAction,
  Tier1Plan,
  VerdictDto,
  WindowKeys,
} from "./wasm-bridge.types";

interface WasmExports {
  install_policies_json(input: string): string;
  build_action_for_request_json(request_json: string): string;
  tier1_fact_plan_json(action_json: string): string;
  tier2_window_keys_json(
    action_json: string,
    oracle_snapshot_json: string,
  ): string;
  evaluate_json(request_json: string, host_snapshot_json: string): string;
  // Phase 7: new 32-variant ActionEnvelope pipeline. Coexists with
  // build_action_for_request_json (legacy 5-variant Action).
  route_request_json(input_json: string): string;
}

/**
 * One ActionEnvelope emitted by the new (Phase 5) pipeline.
 *
 * `action` is the snake_case discriminator (e.g. "swap", "permit",
 * "approve") and `fields` is the variant-specific payload.
 *
 * Field-level typing is intentionally deferred to a follow-up so this
 * minimal binding stays small. Callers that need to introspect specific
 * variants should add per-action zod-style guards as needed.
 */
export interface RouteEnvelope {
  category: string;
  action: string;
  fields: unknown;
}

interface OkEnvelope<T> {
  ok: true;
  data: T;
}
interface ErrEnvelope {
  ok: false;
  error: { kind: string; message: string };
}
type Envelope<T> = OkEnvelope<T> | ErrEnvelope;

export class EngineError extends Error {
  constructor(
    readonly kind: string,
    message: string,
  ) {
    super(`${kind}: ${message}`);
    this.name = "EngineError";
  }
}

let cachedExports: WasmExports | null = null;
let inflightLoad: Promise<WasmExports> | null = null;

const WASM_BG_URL = Browser.runtime.getURL("wasm/policy_engine_wasm_bg.wasm");

async function load(): Promise<WasmExports> {
  if (cachedExports) return cachedExports;
  if (inflightLoad) return inflightLoad;
  inflightLoad = (async () => {
    await init({ module_or_path: WASM_BG_URL });
    cachedExports = wasmExports as unknown as WasmExports;
    return cachedExports;
  })();
  return inflightLoad;
}

function unwrap<T>(json: string): T {
  const parsed = JSON.parse(json) as Envelope<T>;
  if (parsed.ok === true) return parsed.data;
  throw new EngineError(parsed.error.kind, parsed.error.message);
}

export async function installPolicies(input: {
  schema_text: string;
  policy_set: { id: string; text: string }[];
}): Promise<void> {
  const exports = await load();
  unwrap<unknown>(exports.install_policies_json(JSON.stringify(input)));
}

export async function buildActionForRequest(
  request: unknown,
): Promise<ParsedAction> {
  const exports = await load();
  const raw = unwrap<unknown>(
    exports.build_action_for_request_json(JSON.stringify(request)),
  );
  return parseAction(raw);
}

export async function tier1FactPlan(action: unknown): Promise<Tier1Plan> {
  const exports = await load();
  const raw = unwrap<unknown>(
    exports.tier1_fact_plan_json(JSON.stringify(action)),
  );
  return parseTier1Plan(raw);
}

export async function tier2WindowKeys(
  action: unknown,
  oracleEntries: unknown,
): Promise<WindowKeys> {
  const exports = await load();
  const raw = unwrap<unknown>(
    exports.tier2_window_keys_json(
      JSON.stringify(action),
      JSON.stringify(oracleEntries),
    ),
  );
  return parseWindowKeys(raw);
}

export async function evaluate(
  request: unknown,
  snapshot: unknown,
): Promise<VerdictDto> {
  const exports = await load();
  const raw = unwrap<unknown>(
    exports.evaluate_json(JSON.stringify(request), JSON.stringify(snapshot)),
  );
  return parseVerdict(raw);
}

/**
 * Phase 5 pipeline entry. Takes a raw RPC payload and returns the list of
 * `ActionEnvelope` values that the new Decoder/Mapper/CallAdapter or
 * SignAdapter stack produces.
 *
 * Input shape: `{ method, params, chain_id, block_timestamp? }`.
 * Throws `EngineError("route_failed", ...)` when no adapter matched.
 */
export async function routeRequest(input: {
  method: string;
  params: unknown;
  chain_id: number;
  block_timestamp?: number;
}): Promise<RouteEnvelope[]> {
  const exports = await load();
  const raw = unwrap<unknown>(
    exports.route_request_json(JSON.stringify(input)),
  );
  if (!Array.isArray(raw)) {
    throw new EngineError(
      "invalid_route_response",
      `expected ActionEnvelope[] from WASM, got ${typeof raw}`,
    );
  }
  return raw.map((entry, idx) => {
    if (
      typeof entry !== "object" ||
      entry === null ||
      typeof (entry as { category?: unknown }).category !== "string" ||
      typeof (entry as { action?: unknown }).action !== "string"
    ) {
      throw new EngineError(
        "invalid_route_envelope",
        `route_request_json[${idx}] missing required fields`,
      );
    }
    const obj = entry as { category: string; action: string; fields?: unknown };
    return { category: obj.category, action: obj.action, fields: obj.fields };
  });
}
