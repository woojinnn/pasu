/**
 * `/policies` — Cedar policies in the user's `user_policies` table.
 * Server returns the full row including cedar_text + severity flags.
 */

import type { InstalledPolicy, PolicySeverity } from "@scopeball/types";

import { request } from "./client";

export type { InstalledPolicy, PolicySeverity };

/** `GET /policies` — every installed Cedar policy for the user. */
export async function listPolicies(): Promise<InstalledPolicy[]> {
  return request<InstalledPolicy[]>("/policies");
}

export interface CreatePolicyBody {
  name: string;
  description?: string | null;
  cedar_text: string;
  severity: PolicySeverity;
}

/** `POST /policies` — install a new Cedar policy. Returns the new id. */
export async function createPolicy(
  body: CreatePolicyBody,
): Promise<{ id: number; created_at: number }> {
  return request<{ id: number; created_at: number }>("/policies", {
    method: "POST",
    body,
  });
}

export interface PatchPolicyBody {
  name?: string;
  description?: string | null;
  cedar_text?: string;
  severity?: PolicySeverity;
  enabled?: boolean;
}

/** `PATCH /policies/:id` — partial update; absent fields stay. */
export async function patchPolicy(id: number, body: PatchPolicyBody): Promise<void> {
  await request<void>(`/policies/${id}`, { method: "PATCH", body });
}

/** `DELETE /policies/:id` — drop a policy. */
export async function deletePolicy(id: number): Promise<void> {
  await request<void>(`/policies/${id}`, { method: "DELETE" });
}
