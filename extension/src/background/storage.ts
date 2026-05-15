import Browser from "webextension-polyfill";

const AUDIT_KEY = "requests:audit";
const AUDIT_MAX = 100;

/** Inbound request type as carried on the wire from the inpage proxy. */
export type RequestKind =
  | "transaction"
  | "typed-signature"
  | "untyped-signature";

export interface AuditEntry {
  requestId: string;
  hostname: string;
  type: RequestKind;
  bypassed: boolean;
  verdict: "pass" | "warn" | "fail";
  matchedPolicies: { id: string; severity: string }[];
  policyRpc?: {
    request_id: string;
    manifest_set_hash: string;
    schema_hash: string;
    call_ids: string[];
    methods: string[];
  };
  decidedAtMs: number;
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
