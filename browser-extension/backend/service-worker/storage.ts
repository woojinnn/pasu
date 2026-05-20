import Browser from "webextension-polyfill";

const PENDING_KEY = "requests:pending";
const AUDIT_KEY = "requests:audit";
const AUDIT_MAX = 100;

export interface PendingRequest {
  requestId: string;
  hostname: string;
  type: "transaction" | "typed-signature" | "untyped-signature";
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
   * Phase 7F — which Cedar pipeline produced the verdict.
   * `"declarative"` ⇒ `evaluate_with_envelopes_json` (Phase 7A).
   * `"static"` ⇒ legacy `evaluateWithPolicyRpc` path.
   * Absent on engine-error short-circuits (where we have no signal).
   */
  verdictSource?: "declarative" | "static";
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
