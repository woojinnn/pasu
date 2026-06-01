/**
 * Static catalog endpoints — policy schema, policy templates, example
 * transactions, spender reputation. All served from the server's
 * embedded JSON / toml; responses are cacheable (Cache-Control: 300s).
 */

import type { Address, I18nString, PolicySeverity } from "./types";

import { request } from "./client";

// ---------- /policy-schema ----------

export interface PolicyPredicate {
  key: string;
  group: "address" | "numeric" | "enum" | "boolean" | "misc" | string;
  type: string;
  ops: string[];
  label: I18nString;
  note?: I18nString;
  unit?: string;
  derived?: boolean;
}

export interface PolicyActionMeta {
  id: string;
  domain: string;
  kind: string;
  label: I18nString;
}

export interface PolicySchema {
  $schema: string;
  actions: PolicyActionMeta[];
  predicates: PolicyPredicate[];
  operators: Record<string, string[]>;
  roles: Record<string, { tone: string; icon: string }>;
}

export async function getPolicySchema(): Promise<PolicySchema> {
  return request<PolicySchema>("/policy-schema");
}

// ---------- /policy-templates ----------

export interface PolicyTemplate {
  id: string;
  name: I18nString;
  description: I18nString;
  severity: PolicySeverity;
  cedar_text: string;
}

export async function getPolicyTemplates(): Promise<PolicyTemplate[]> {
  return request<PolicyTemplate[]>("/policy-templates");
}

// ---------- /examples/transactions ----------

export interface ExampleTransaction {
  id: string;
  label: I18nString;
  action: unknown;
  meta: Record<string, unknown>;
  context?: Record<string, unknown>;
  enrichment?: Record<string, unknown>;
  expected?: { verdict: "pass" | "warn" | "deny"; failedGuards?: string[] };
}

export async function getExampleTransactions(): Promise<ExampleTransaction[]> {
  return request<ExampleTransaction[]>("/examples/transactions");
}

// ---------- legacy policy detail ----------

import type { InstalledPolicy } from "./types";

export async function getPolicy(id: number): Promise<InstalledPolicy> {
  void id;
  throw new Error("server policy detail has moved to extension-local storage");
}

// ---------- /spenders/:addr ----------

export type SpenderRep = "known" | "blocked";

export interface SpenderMeta {
  addr: Address;
  label: string;
  rep: SpenderRep;
  chain?: string;
  notes?: string;
}

/** Returns `null` for addresses not in the catalog (server 404 = "unknown"). */
export async function getSpender(addr: Address): Promise<SpenderMeta | null> {
  try {
    return await request<SpenderMeta>(`/spenders/${addr}`);
  } catch (e) {
    // Lazy import to avoid coupling client.ts here.
    if (
      e &&
      typeof e === "object" &&
      "status" in e &&
      (e as { status: number }).status === 404
    ) {
      return null;
    }
    throw e;
  }
}
