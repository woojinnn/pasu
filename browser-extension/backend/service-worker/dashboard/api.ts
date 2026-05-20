import {
  type ParamsSchema,
  type ParamValues,
  validateParams,
} from "../marketplace/params-validator";
import { renderAndVerify } from "../marketplace/template-renderer";
import { reinstallAllPolicies } from "../policies-loader";
import {
  applyEnabledIds,
  type ApplyResult,
  getCatalog,
  getEnabledIds,
} from "../policy-selection";
import { auditRead, type AuditEntry } from "../storage";
import {
  DASHBOARD_ID_PREFIX,
  type ManagedPolicy,
  deleteManaged,
  listManaged,
  upsertManaged,
} from "./storage";

// Hard ceiling for audit-log responses so a wedged dashboard can't pull
// the entire ring buffer in one shot. The underlying buffer is capped at
// AUDIT_MAX (100) so this is mostly defensive.
const AUDIT_DEFAULT_LIMIT = 100;
const AUDIT_MAX_LIMIT = 200;

const RULE_KEYWORD_RE = /\b(forbid|permit)\s*\(/;

export type DashboardRequest =
  | { type: "dashboard:ping" }
  | { type: "dashboard:list-managed" }
  | { type: "dashboard:get-catalog" }
  | {
      type: "dashboard:put-raw";
      id: string;
      text: string;
      manifest?: unknown;
      manifests?: readonly unknown[];
    }
  | {
      type: "dashboard:put-template";
      id: string;
      templateText: string;
      paramsSchema: ParamsSchema;
      paramValues: ParamValues;
      manifest?: unknown;
      manifests?: readonly unknown[];
    }
  | { type: "dashboard:delete"; id: string }
  | { type: "dashboard:set-enabled-ids"; ids: string[] }
  | {
      type: "dashboard:get-audit-log";
      opts?: {
        limit?: number;
        since?: number;
      };
    };

export type DashboardResponse<T = unknown> =
  | { ok: true; data: T }
  | { ok: false; error: { kind: string; message: string } };

export function isDashboardRequest(value: unknown): value is DashboardRequest {
  if (!value || typeof value !== "object") return false;
  const t = (value as { type?: unknown }).type;
  return typeof t === "string" && t.startsWith("dashboard:");
}

function fail(kind: string, message: string): DashboardResponse {
  return { ok: false, error: { kind, message } };
}

function classify(err: unknown): { kind: string; message: string } {
  if (err instanceof Error) {
    const m = err.message.match(/^([a-z_]+):\s*(.*)$/);
    if (m) return { kind: m[1], message: m[2] };
    return { kind: "dashboard_failed", message: err.message };
  }
  return { kind: "dashboard_failed", message: String(err) };
}

/**
 * On any put/delete, auto-extend the enabled set with the new id (or remove
 * the deleted one) and run `applyEnabledIds(reinstallAllPolicies)`. This keeps
 * "store changed → engine reflects it" as a single, serialized hop — the
 * dashboard never has to know about the apply queue.
 */
async function autoApplyEnabled(
  changeFn: (current: Set<string>) => Set<string>,
): Promise<ApplyResult> {
  const current = new Set(await getEnabledIds());
  const next = changeFn(current);
  const nextIds = [...next];
  return applyEnabledIds(nextIds, reinstallAllPolicies);
}

/**
 * Persist + apply + rollback wrapper. Used by put-raw and put-template so a
 * policy whose Cedar (or schema) fails WASM validation doesn't leave the store
 * in a broken state.
 *
 * Sequence (success path):
 *   1. Snapshot prior storage + enabled-set state.
 *   2. upsertManaged(policy)
 *   3. applyEnabledIds(currentEnabled ∪ {id}, reinstall) — WASM validates here.
 *   4. Read catalog and return { policy, catalog }.
 *
 * Sequence (failure path — WASM rejected):
 *   3a. Restore the prior managed-policy entry (or remove if it was a fresh
 *       insert).
 *   3b. applyEnabledIds(priorEnabled, reinstall) so ENABLED/APPLIED keys and
 *       the running engine snap back to the last good state. The prior set
 *       was working before this call, so this reinstall must succeed unless
 *       the storage was already broken — in that pathological case we still
 *       surface the original error.
 */
async function persistThenApply(
  policy: ManagedPolicy,
): Promise<DashboardResponse<{ policy: ManagedPolicy; catalog: unknown }>> {
  const priorList = await listManaged();
  const priorEntry = priorList.find((p) => p.id === policy.id);
  const priorEnabled = await getEnabledIds();

  await upsertManaged(policy);
  const apply = await autoApplyEnabled((cur) => {
    cur.add(policy.id);
    return cur;
  });

  if (!apply.ok) {
    // Roll storage back to its pre-call shape.
    if (priorEntry) {
      await upsertManaged(priorEntry);
    } else {
      await deleteManaged(policy.id);
    }
    // Snap the engine back to the prior enabled set. Best-effort; if this
    // also fails, surface the *original* error rather than the rollback's.
    await applyEnabledIds(priorEnabled, reinstallAllPolicies).catch(
      () => undefined,
    );
    return { ok: false, error: apply.error };
  }

  const catalog = await getCatalog();
  return { ok: true, data: { policy, catalog } };
}

export async function handleDashboardRequest(
  req: DashboardRequest,
): Promise<DashboardResponse> {
  try {
    switch (req.type) {
      case "dashboard:ping": {
        return { ok: true, data: { version: 1 } };
      }

      case "dashboard:list-managed": {
        const list = await listManaged();
        return { ok: true, data: list };
      }

      case "dashboard:get-catalog": {
        const cat = await getCatalog();
        return { ok: true, data: cat };
      }

      case "dashboard:put-raw": {
        if (typeof req.id !== "string" || typeof req.text !== "string") {
          return fail("invalid_request", "id and text must be strings");
        }
        // Pre-filter obvious garbage. The WASM engine does real validation
        // when policies are installed; this is just so the storage can't
        // accumulate text that has no chance of ever being a Cedar policy.
        if (!RULE_KEYWORD_RE.test(req.text)) {
          return fail(
            "parse_failed",
            "policy text contains no forbid/permit rule",
          );
        }
        const policy: ManagedPolicy = {
          id: req.id,
          kind: "raw",
          text: req.text,
          ...(req.manifest !== undefined ? { manifest: req.manifest } : {}),
          ...(req.manifests !== undefined ? { manifests: req.manifests } : {}),
          updatedAtMs: Date.now(),
          schemaVersion: 1,
        };
        return await persistThenApply(policy);
      }

      case "dashboard:put-template": {
        if (
          typeof req.id !== "string" ||
          typeof req.templateText !== "string" ||
          !req.paramsSchema ||
          typeof req.paramsSchema !== "object" ||
          !req.paramValues ||
          typeof req.paramValues !== "object"
        ) {
          return fail(
            "invalid_request",
            "id, templateText, paramsSchema, paramValues required",
          );
        }
        validateParams(req.paramsSchema, req.paramValues);
        const rendered = renderAndVerify({
          policyId: req.id,
          templateText: req.templateText,
          paramsSchema: req.paramsSchema,
          paramValues: req.paramValues,
        });
        const policy: ManagedPolicy = {
          id: req.id,
          kind: "template",
          text: rendered,
          template: {
            source: req.templateText,
            paramsSchema: req.paramsSchema,
            paramValues: req.paramValues,
          },
          ...(req.manifest !== undefined ? { manifest: req.manifest } : {}),
          ...(req.manifests !== undefined ? { manifests: req.manifests } : {}),
          updatedAtMs: Date.now(),
          schemaVersion: 1,
        };
        return await persistThenApply(policy);
      }

      case "dashboard:delete": {
        if (typeof req.id !== "string" || !req.id.startsWith(DASHBOARD_ID_PREFIX)) {
          return fail("invalid_request", "id must be a dashboard:: id");
        }
        await deleteManaged(req.id);
        const apply = await autoApplyEnabled((cur) => {
          cur.delete(req.id);
          return cur;
        });
        if (!apply.ok) return { ok: false, error: apply.error };
        const catalog = await getCatalog();
        return { ok: true, data: { catalog } };
      }

      case "dashboard:set-enabled-ids": {
        if (
          !Array.isArray(req.ids) ||
          !req.ids.every((id) => typeof id === "string")
        ) {
          return fail("invalid_request", "ids must be string[]");
        }
        const result = await applyEnabledIds(req.ids, reinstallAllPolicies);
        if (!result.ok) return { ok: false, error: result.error };
        const catalog = await getCatalog();
        return { ok: true, data: { catalog } };
      }

      case "dashboard:get-audit-log": {
        const raw = await auditRead();
        const opts = req.opts ?? {};
        let entries: AuditEntry[] = raw;
        if (typeof opts.since === "number") {
          entries = entries.filter((e) => e.decidedAtMs >= opts.since!);
        }
        // Most-recent-first; storage appends so reverse() gives newest first.
        entries = [...entries].reverse();
        const requested =
          typeof opts.limit === "number" && opts.limit > 0
            ? Math.min(opts.limit, AUDIT_MAX_LIMIT)
            : AUDIT_DEFAULT_LIMIT;
        return { ok: true, data: entries.slice(0, requested) };
      }

      default: {
        const _exhaustive: never = req;
        void _exhaustive;
        return fail("unknown_request", `unrecognized dashboard request`);
      }
    }
  } catch (err) {
    return { ok: false, error: classify(err) };
  }
}
