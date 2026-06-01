import type { InstalledPolicy, PolicySeverity } from "./types";

export type { InstalledPolicy };

export async function listPolicies(): Promise<InstalledPolicy[]> {
  return [];
}

export interface CreatePolicyBody {
  name: string;
  description?: string | null;
  cedar_text: string;
  severity: PolicySeverity;
}

export async function createPolicy(
  _body: CreatePolicyBody,
): Promise<{ id: number; created_at: number }> {
  throw new Error("server policy CRUD has moved to extension-local storage");
}

export interface PatchPolicyBody {
  name?: string;
  description?: string | null;
  cedar_text?: string;
  severity?: PolicySeverity;
  enabled?: boolean;
}

export async function patchPolicy(
  _id: number,
  _body: PatchPolicyBody,
): Promise<void> {
  throw new Error("server policy CRUD has moved to extension-local storage");
}

export async function deletePolicy(_id: number): Promise<void> {
  throw new Error("server policy CRUD has moved to extension-local storage");
}
