import { pathToFileURL } from "node:url";

import { createPolicyRpcServer, type PolicyRpcServerOptions } from "./server";

export interface StartPolicyRpcServerOptions extends PolicyRpcServerOptions {
  host?: string;
  port?: number;
}

export function startPolicyRpcServer(options: StartPolicyRpcServerOptions = {}) {
  const host = options.host ?? "127.0.0.1";
  const port = options.port ?? 8787;
  const server = createPolicyRpcServer(options);

  server.listen(port, host, () => {
    console.log(`policy-rpc listening on http://${host}:${port}`);
  });

  return server;
}

const entrypointUrl = process.argv[1] ? pathToFileURL(process.argv[1]).href : "";

if (import.meta.url === entrypointUrl) {
  startPolicyRpcServer({
    host: process.env.HOST ?? "127.0.0.1",
    port: Number.parseInt(process.env.PORT ?? "8787", 10),
  });
}
