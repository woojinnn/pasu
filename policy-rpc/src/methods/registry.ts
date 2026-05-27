import {
  createOracleUsdValueMethod,
  oracleUsdValueCatalog,
  type OracleUsdValueMethodOptions,
} from "./oracle-usd-value.js";
import {
  approvalAllowanceCatalog,
  approvalCoverInputsCatalog,
  clockNowCatalog,
  createApprovalAllowanceMethod,
  createApprovalCoverInputsMethod,
  createClockNowMethod,
  createOracleEffectiveRateBpsMethod,
  createPortfolioBalanceMethod,
  createPortfolioInputFractionBpsMethod,
  createStatWindowSnapshotMethod,
  createStatWindowSwapStatsMethod,
  oracleEffectiveRateBpsCatalog,
  portfolioBalanceCatalog,
  portfolioInputFractionBpsCatalog,
  statWindowSnapshotCatalog,
  statWindowSwapStatsCatalog,
} from "./mock-host-capabilities.js";
import {
  createScopeballEvaluateV3Method,
  scopeballEvaluateV3Catalog,
} from "./scopeball-evaluate-v3.js";
import {
  RpcMethodError,
  type JsonObject,
  type PolicyRpcCall,
  type RpcResult,
} from "../types.js";
import type { MethodCatalog, MethodCatalogEntry } from "./catalog.js";
import type { LoadedPluginEntry } from "./plugin-loader.js";
import type { LoadedSidecarEntry } from "./sidecar-loader.js";

export type RpcMethod = (params: unknown) => Promise<JsonObject>;

export interface MethodRegistry {
  /** Names only — kept for backwards compatibility with older callers. */
  listMethods(): string[];
  /**
   * Full catalog (params, returns, origin tags) keyed by method name.
   * `GET /v1/methods` returns this verbatim; the dashboard's manifest
   * editor consumes it to drive method/param/output dropdowns.
   */
  catalog(): MethodCatalog;
  execute(call: PolicyRpcCall): Promise<RpcResult>;
}

export interface MethodRegistryOptions extends OracleUsdValueMethodOptions {
  /**
   * Extra in-process plugin entries to merge alongside the bundled set.
   * Typically populated by the server's startup hook from
   * `loadPluginEntries`; tests can pass synthetic entries.
   */
  pluginEntries?: readonly LoadedPluginEntry[];
  /** Same idea for sidecar-forwarded methods. */
  sidecarEntries?: readonly LoadedSidecarEntry[];
  /**
   * Warning sink for plugin/sidecar conflict reports. Defaults to
   * `console.warn`. Tests override to capture the message stream
   * without polluting stdout.
   */
  warn?: (message: string, ...args: unknown[]) => void;
}

interface MethodEntry {
  fn: RpcMethod;
  catalog: MethodCatalogEntry;
}

export function createMethodRegistry(options: MethodRegistryOptions = {}): MethodRegistry {
  // (name, factory, catalog) triples — single source of order. The
  // catalog and the dispatch table can never drift because they're
  // declared in one place. Anyone adding a method has to touch all
  // three columns of one row.
  const entries: MethodEntry[] = [
    {
      fn: createApprovalAllowanceMethod() as RpcMethod,
      catalog: approvalAllowanceCatalog,
    },
    {
      fn: createApprovalCoverInputsMethod() as RpcMethod,
      catalog: approvalCoverInputsCatalog,
    },
    {
      fn: createClockNowMethod(options.nowMs) as RpcMethod,
      catalog: clockNowCatalog,
    },
    {
      fn: createOracleEffectiveRateBpsMethod() as RpcMethod,
      catalog: oracleEffectiveRateBpsCatalog,
    },
    {
      fn: createOracleUsdValueMethod(options) as RpcMethod,
      catalog: oracleUsdValueCatalog,
    },
    {
      fn: createPortfolioBalanceMethod() as RpcMethod,
      catalog: portfolioBalanceCatalog,
    },
    {
      fn: createPortfolioInputFractionBpsMethod() as RpcMethod,
      catalog: portfolioInputFractionBpsCatalog,
    },
    {
      fn: createStatWindowSnapshotMethod() as RpcMethod,
      catalog: statWindowSnapshotCatalog,
    },
    {
      fn: createStatWindowSwapStatsMethod() as RpcMethod,
      catalog: statWindowSwapStatsCatalog,
    },
    // Phase 5D — `scopeball.evaluate_v3` echo mock. Real reducer +
    // state-sync implementation lands in Phase 6.
    {
      fn: createScopeballEvaluateV3Method() as RpcMethod,
      catalog: scopeballEvaluateV3Catalog,
    },
  ];

  const methods = new Map<string, MethodEntry>(
    entries.map((e) => [e.catalog.name, e]),
  );

  // Merge plugin + sidecar entries on top of the bundled set.
  //
  // Conflict resolution: bundled methods always win. A plugin or
  // sidecar that declares the same name as a bundled method gets
  // rejected and logged. We deliberately do NOT let extensions
  // override `oracle.usd_value` etc. — the dashboard's bundled
  // catalog.json would diverge from the live registry and the
  // manifest editor would offer the wrong contract for that name.
  const warn = options.warn ?? console.warn;
  for (const plugin of options.pluginEntries ?? []) {
    if (methods.has(plugin.catalog.name)) {
      warn(
        `[policy-rpc] plugin at ${plugin.source} declared "${plugin.catalog.name}" but a bundled method already owns that name; ignoring plugin entry`,
      );
      continue;
    }
    methods.set(plugin.catalog.name, {
      fn: plugin.fn,
      catalog: plugin.catalog,
    });
  }
  for (const sidecar of options.sidecarEntries ?? []) {
    if (methods.has(sidecar.catalog.name)) {
      warn(
        `[policy-rpc] sidecar ${sidecar.source.name} declared "${sidecar.catalog.name}" but it's already registered (bundled or plugin); ignoring sidecar entry`,
      );
      continue;
    }
    methods.set(sidecar.catalog.name, {
      fn: sidecar.fn,
      catalog: sidecar.catalog,
    });
  }

  return {
    listMethods: () => [...methods.keys()].sort(),

    catalog: () => {
      const out: MethodCatalog = { methods: {} };
      // Insert in sorted order so the JSON the server emits is stable
      // — easier to diff in tests + nicer to read by hand.
      for (const name of [...methods.keys()].sort()) {
        out.methods[name] = methods.get(name)!.catalog;
      }
      return out;
    },

    async execute(call: PolicyRpcCall): Promise<RpcResult> {
      const entry = methods.get(call.method);

      if (!entry) {
        return {
          id: call.id,
          ok: false,
          error: {
            code: "method_not_found",
            message: `Unknown method ${call.method}`,
          },
        };
      }

      try {
        const result = await entry.fn(call.params);

        return {
          id: call.id,
          ok: true,
          result,
        };
      } catch (error) {
        const methodError = normalizeMethodError(error);

        return {
          id: call.id,
          ok: false,
          error: {
            code: methodError.code,
            message: methodError.message,
          },
        };
      }
    },
  };
}

function normalizeMethodError(error: unknown): RpcMethodError {
  if (error instanceof RpcMethodError) {
    return error;
  }

  if (error instanceof Error) {
    return new RpcMethodError("internal_error", error.message);
  }

  return new RpcMethodError("internal_error", "Unknown method error");
}
