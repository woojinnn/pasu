/**
 * `/policies` — Cedar policies in the user's `user_policies` table.
 * Phase 9+ endpoint; the Rust server returns the full row including
 * cedar_text + severity flags.
 */

import { request } from "./client";

export interface InstalledPolicy {
  id: number;
  name: string;
  description: string | null;
  cedar_text: string;
  severity: string; // "deny" | "warn" | "info"
  enabled: boolean;
  created_at: number;
  updated_at: number;
}

/** `GET /policies` — every installed Cedar policy for the user. */
export async function listPolicies(): Promise<InstalledPolicy[]> {
  return request<InstalledPolicy[]>("/policies");
}
