import Browser from "webextension-polyfill";

const PENDING_KEY = "requests:pending";
const AUDIT_KEY = "requests:audit";
const AUDIT_MAX = 100;

export interface PendingRequest {
  requestId: string;
  hostname: string;
  type:
    | "transaction"
    | "typed-signature"
    | "untyped-signature"
    | "venue-order";
  bypassed: boolean;
  envelope: unknown; // redacted summary; raw body in IndexedDB if ever stored
  enqueuedAtMs: number;
}

export interface AuditEntry {
  requestId: string;
  hostname: string;
  type: PendingRequest["type"];
  bypassed: boolean;
  verdict: "pass" | "warn" | "fail";
  matchedPolicies: { id: string; severity: string; reason?: string }[];
  policyRpc?: {
    request_id: string;
    manifest_set_hash: string;
    schema_hash: string;
    call_ids: string[];
    methods: string[];
  };
  /**
   * Phase 6 — declarative adapter pipeline audit, only present for
   * transactions (the only message type the declarative path runs for).
   * See `orchestrator.ts::DeclarativeAuditMeta` for the contract.
   */
  declarative?: {
    outcome: "hit" | "miss" | "fault";
    source?: "layer1" | "layer2" | "jit";
    decoder_id?: string;
    bundle_id?: string;
    envelope_count?: number;
    reason?: string;
  };
  /**
   * v3 declarative / ActionBody pipeline audit. For onchain tx and typed
   * signatures this reflects registry-v3 routing. For venue orders this
   * records the HyperLiquid ActionBody conversion/evaluation branch.
   * See `orchestrator.ts::DeclarativeV3AuditMeta` for the contract.
   */
  declarativeV3?: {
    outcome: "hit" | "miss" | "fault";
    nature: "onchain_tx" | "offchain_sig" | "untyped_sig";
    decoder_id?: string;
    action_count?: number;
    reason?: string;
  };
  /**
   * Phase 1 / P3 — which pipeline produced the verdict.
   * `"declarative-v2"` ⇒ the stateless v2 pipeline
   *   (`plan_action_rpc_v2_json` → host dispatch → `evaluate_action_v2_json`).
   * `"fail_closed"` ⇒ no decoder produced an evaluable verdict (v3 miss/fault,
   *   all-`Unknown` bodies, no v2 bundles, a v2 throw, typed-signature
   *   route/evaluate miss, the untyped-signature short-circuit, venue-order
   *   deny-closed paths, or the hard-timeout fallback).
   * Absent on engine-error short-circuits (where we have no signal).
   */
  verdictSource?: "declarative-v2" | "fail_closed";
  decidedAtMs: number;
}

export async function pendingPut(req: PendingRequest): Promise<void> {
  const stored =
    ((
      (await Browser.storage.session.get(PENDING_KEY)) as Record<
        string,
        unknown
      >
    )[PENDING_KEY] as Record<string, PendingRequest> | undefined) ?? {};
  stored[req.requestId] = req;
  await Browser.storage.session.set({ [PENDING_KEY]: stored });
}

export async function pendingGet(
  requestId: string,
): Promise<PendingRequest | undefined> {
  const stored =
    ((
      (await Browser.storage.session.get(PENDING_KEY)) as Record<
        string,
        unknown
      >
    )[PENDING_KEY] as Record<string, PendingRequest> | undefined) ?? {};
  return stored[requestId];
}

export async function pendingDelete(requestId: string): Promise<void> {
  const stored =
    ((
      (await Browser.storage.session.get(PENDING_KEY)) as Record<
        string,
        unknown
      >
    )[PENDING_KEY] as Record<string, PendingRequest> | undefined) ?? {};
  delete stored[requestId];
  await Browser.storage.session.set({ [PENDING_KEY]: stored });
}

export async function auditAppend(entry: AuditEntry): Promise<void> {
  const log =
    (((await Browser.storage.local.get(AUDIT_KEY)) as Record<string, unknown>)[
      AUDIT_KEY
    ] as AuditEntry[] | undefined) ?? [];
  log.push(entry);
  if (log.length > AUDIT_MAX) log.splice(0, log.length - AUDIT_MAX);
  await Browser.storage.local.set({ [AUDIT_KEY]: log });
}

export async function auditRead(): Promise<AuditEntry[]> {
  const log =
    (((await Browser.storage.local.get(AUDIT_KEY)) as Record<string, unknown>)[
      AUDIT_KEY
    ] as AuditEntry[] | undefined) ?? [];
  return log;
}
