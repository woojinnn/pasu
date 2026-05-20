/**
 * registry-api — process entrypoint.
 *
 * Importable (테스트는 createRegistryApiServer 를 직접 씀) + runnable
 * (import.meta.url === argv[1] guard 가 listener 시작). Cloud Run 이
 * PORT (8080) 를 주입하고 0.0.0.0 bind 를 기대.
 */
import { pathToFileURL } from "node:url";
import { loadConfig } from "./config.js";
import { GcsObjectReader } from "./gcs-client.js";
import { createRegistryApiServer } from "./server.js";

export function startRegistryApiServer() {
  const config = loadConfig();
  const reader = new GcsObjectReader({ bucketName: config.bucketName });
  const server = createRegistryApiServer({ config, reader });
  server.listen(config.port, config.host, () => {
    console.log(
      JSON.stringify({
        event: "registry_api_listening",
        host: config.host,
        port: config.port,
        bucket: config.bucketName,
      }),
    );
  });
  return server;
}

const entrypointUrl = process.argv[1]
  ? pathToFileURL(process.argv[1]).href
  : "";
if (import.meta.url === entrypointUrl) {
  startRegistryApiServer();
}
