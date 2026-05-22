import { pathToFileURL } from "node:url";

import {
  bootstrapPolicyRpcServer,
  createPolicyRpcServer,
  readSidecarConfigFile,
  type PolicyRpcServerOptions,
} from "./server.js";

export interface StartPolicyRpcServerOptions extends PolicyRpcServerOptions {
  host?: string;
  port?: number;
}

/**
 * Synchronous start — keeps the original semantics for callers that
 * supply their own registry (notably tests) and don't want async
 * plugin/sidecar discovery to delay listen().
 */
export function startPolicyRpcServer(options: StartPolicyRpcServerOptions = {}) {
  const host = options.host ?? "127.0.0.1";
  const port = options.port ?? 8787;
  const server = createPolicyRpcServer(options);

  server.listen(port, host, () => {
    console.log(`policy-rpc listening on http://${host}:${port}`);
  });

  return server;
}

/**
 * Async start — runs plugin + sidecar discovery first, then listens.
 * The production entry point uses this so the registry surfaces
 * everything `GET /v1/methods` is expected to return.
 *
 * Plugins come from `POLICY_RPC_PLUGINS_DIR` (default `./plugins`);
 * sidecars come from `POLICY_RPC_SIDECARS` (path to a JSON config
 * file, default `./policy-rpc-sidecars.json`). Both default paths
 * fail-open on absence so the daemon still starts in fresh installs.
 */
export async function startPolicyRpcServerWithDiscovery(
  options: StartPolicyRpcServerOptions = {},
) {
  const host = options.host ?? "127.0.0.1";
  const port = options.port ?? 8787;

  const sidecarConfigPath =
    process.env.POLICY_RPC_SIDECARS ?? "./policy-rpc-sidecars.json";
  const sidecars = await readSidecarConfigFile(sidecarConfigPath);

  const pluginsDir = process.env.POLICY_RPC_PLUGINS_DIR;
  const { server, pluginEntries, sidecarEntries } = await bootstrapPolicyRpcServer({
    ...options,
    plugins: pluginsDir ? { dir: pluginsDir } : {},
    sidecars: { sidecars },
  });

  server.listen(port, host, () => {
    const pluginCount = pluginEntries.length;
    const sidecarCount = sidecarEntries.length;
    console.log(
      `policy-rpc listening on http://${host}:${port} ` +
        `(plugins: ${pluginCount}, sidecars: ${sidecarCount})`,
    );
  });

  return server;
}

const entrypointUrl = process.argv[1] ? pathToFileURL(process.argv[1]).href : "";

if (import.meta.url === entrypointUrl) {
  void startPolicyRpcServerWithDiscovery({
    host: process.env.HOST ?? "127.0.0.1",
    port: Number.parseInt(process.env.PORT ?? "8787", 10),
  });
}
