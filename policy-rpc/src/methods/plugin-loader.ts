// In-process plugin loader (X-2 model).
//
// At daemon startup, scan a configured directory for `.js`/`.mjs`
// plugin files and dynamically import each. Each plugin's default
// export is validated against the `InProcessPlugin` shape and
// registered as a method.
//
// Errors during plugin load are NEVER fatal — a broken plugin must
// not stop the daemon from serving its bundled methods. We log the
// failure and skip; the operator sees the warning in logs and can
// fix or remove the offending plugin file.
//
// The default plugin directory is `./plugins` relative to the
// daemon's working directory; override via the
// `POLICY_RPC_PLUGINS_DIR` env var. An absent directory is normal
// (most installs ship no plugins) and silent — only a present
// directory that fails to read produces a warning.

import { readdir } from "node:fs/promises";
import { pathToFileURL } from "node:url";
import { resolve } from "node:path";
import { existsSync, statSync } from "node:fs";

import type {
  InProcessPlugin,
  MethodCatalogEntry,
} from "./catalog.js";
import type { JsonObject } from "../types.js";

/** Method entry shape the registry consumes (mirrors registry.ts). */
export interface LoadedPluginEntry {
  fn: (params: unknown) => Promise<JsonObject>;
  catalog: MethodCatalogEntry;
  /** Filesystem path the plugin was loaded from (for error reporting). */
  source: string;
}

export interface PluginLoaderOptions {
  /** Absolute or daemon-relative path. Default: `./plugins`. */
  dir?: string;
  /**
   * Optional override of the directory listing function for tests so
   * we don't have to scaffold real files. Has the same shape as
   * `node:fs/promises::readdir`.
   */
  readdir?: typeof readdir;
  /**
   * Optional override of the dynamic-import call so tests can inject
   * synthetic plugins. Takes a URL string, returns the module.
   */
  importModule?: (url: string) => Promise<unknown>;
  /** Override the existsSync check (test seam). */
  exists?: (path: string) => boolean;
  /** Sink for warnings. Defaults to `console.warn`. */
  warn?: (message: string, ...args: unknown[]) => void;
}

const DEFAULT_DIR = "./plugins";

/**
 * Scan the plugin directory and return every successfully-loaded
 * plugin as a registry-compatible entry. Failures are warned and
 * skipped — partial success is the success criterion here.
 */
export async function loadPluginEntries(
  options: PluginLoaderOptions = {},
): Promise<LoadedPluginEntry[]> {
  const dir = resolve(options.dir ?? process.env.POLICY_RPC_PLUGINS_DIR ?? DEFAULT_DIR);
  const exists = options.exists ?? defaultExists;
  const warn = options.warn ?? console.warn;
  const dirRead = options.readdir ?? readdir;
  const importModule = options.importModule ?? defaultImport;

  if (!exists(dir)) {
    // Absent dir is the most common case (no plugins shipped) — quiet.
    return [];
  }

  let files: string[];
  try {
    files = await dirRead(dir);
  } catch (error) {
    warn(`[policy-rpc] plugin dir read failed at ${dir}: ${asMessage(error)}`);
    return [];
  }

  const out: LoadedPluginEntry[] = [];
  for (const file of files) {
    if (!isPluginFile(file)) continue;
    const fullPath = resolve(dir, file);
    const entry = await tryLoadOne(fullPath, importModule, warn);
    if (entry) out.push(entry);
  }
  return out;
}

function isPluginFile(name: string): boolean {
  if (name.startsWith("_") || name.startsWith(".")) return false;
  return name.endsWith(".js") || name.endsWith(".mjs") || name.endsWith(".cjs");
}

async function tryLoadOne(
  fullPath: string,
  importModule: (url: string) => Promise<unknown>,
  warn: (message: string, ...args: unknown[]) => void,
): Promise<LoadedPluginEntry | null> {
  let mod: unknown;
  try {
    mod = await importModule(pathToFileURL(fullPath).href);
  } catch (error) {
    warn(`[policy-rpc] plugin import failed for ${fullPath}: ${asMessage(error)}`);
    return null;
  }

  const plugin = extractDefaultExport(mod);
  const issue = validatePlugin(plugin);
  if (issue !== null) {
    warn(`[policy-rpc] plugin at ${fullPath} rejected: ${issue}`);
    return null;
  }
  const typed = plugin as InProcessPlugin;
  if (typed.catalog.origin !== "plugin") {
    warn(
      `[policy-rpc] plugin at ${fullPath} has origin=${typed.catalog.origin}; coercing to "plugin"`,
    );
  }
  return {
    fn: typed.execute,
    // Force origin so a plugin can't impersonate `bundled` and confuse
    // the dashboard's badge logic.
    catalog: { ...typed.catalog, origin: "plugin" },
    source: fullPath,
  };
}

function extractDefaultExport(mod: unknown): unknown {
  if (mod && typeof mod === "object") {
    const asObj = mod as { default?: unknown };
    if ("default" in asObj) return asObj.default;
  }
  return mod;
}

/**
 * Return `null` when the value satisfies `InProcessPlugin`, otherwise
 * a short description of why it doesn't (used in warn() output).
 */
function validatePlugin(value: unknown): string | null {
  if (!value || typeof value !== "object") {
    return "default export is not an object";
  }
  const p = value as Partial<InProcessPlugin>;
  if (!p.catalog || typeof p.catalog !== "object") {
    return "missing `catalog` field";
  }
  const cat = p.catalog as Partial<MethodCatalogEntry>;
  if (typeof cat.name !== "string" || cat.name.length === 0) {
    return "catalog.name must be a non-empty string";
  }
  if (typeof p.execute !== "function") {
    return "missing `execute` function";
  }
  if (!cat.params || typeof cat.params !== "object") {
    return "catalog.params must be an object";
  }
  if (!cat.returns || typeof cat.returns !== "object") {
    return "catalog.returns must be an object";
  }
  return null;
}

function defaultExists(path: string): boolean {
  try {
    return existsSync(path) && statSync(path).isDirectory();
  } catch {
    return false;
  }
}

async function defaultImport(url: string): Promise<unknown> {
  return await import(url);
}

function asMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
