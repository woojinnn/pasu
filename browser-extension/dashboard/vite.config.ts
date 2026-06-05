import { defineConfig, loadEnv } from "vite";
import react from "@vitejs/plugin-react";
import path from "node:path";

// Two output modes share this single config:
//
//   dev (`vite` / `yarn dev`): standalone SPA at http://127.0.0.1:5173.
//     5173 is Vite's documented default — keep it so every "how do I
//     hit the dashboard?" reference in the repo lines up.
//     The extension's `dashboard-bridge` content script is pinned to
//     the same port so the SDK in-page proxy works.
//
//   extension build (`vite build` / `vite build --watch`): emits
//     `options.html` + hashed assets straight into `../dist/chrome/`,
//     side-by-side with the webpack-built popup / SW / content-scripts.
//     `base: "./"` is required so the bundled <script src=…> resolves
//     under `chrome-extension://<id>/assets/…` instead of `/assets/…`.
//     `emptyOutDir: false` preserves the webpack output that ran first.
//
// IMPORTANT: when wiring scripts, run webpack BEFORE vite. The pages
// webpack config has `clean: true`, which would wipe the vite output.
export default defineConfig(({ mode }) => {
  // Server base URL is UNIFIED with the webpack (service-worker) build:
  // both read `PASU_SERVER_URL`, so a single env var switches the whole
  // extension (dashboard + service worker) between local/test and prod —
  //   PASU_SERVER_URL=https://pasu-policy.duckdns.org yarn build:ext
  // `loadEnv(mode, dir, "")` reads .env files + process.env with no prefix
  // filter; legacy `VITE_PASU_SERVER_URL` is still honored as a fallback.
  const env = loadEnv(mode, process.cwd(), "");
  const serverUrl =
    env.PASU_SERVER_URL || env.VITE_PASU_SERVER_URL || "";

  return {
    plugins: [react()],
    base: "./",
    // Feed the unified server URL to the dashboard client (client.ts reads
    // `import.meta.env.VITE_PASU_SERVER_URL`).
    define: {
      "import.meta.env.VITE_PASU_SERVER_URL": JSON.stringify(serverUrl),
    },
    build: {
      outDir: path.resolve(__dirname, "../dist/chrome"),
      emptyOutDir: false,
      rollupOptions: {
        input: {
          // Entry name becomes the html filename, so this produces
          // dist/chrome/options.html — referenced from manifest.json
          // as `options_page`.
          options: path.resolve(__dirname, "options.html"),
        },
      },
    },
    server: {
      port: 5173,
      strictPort: true,
      // Node 17+ resolves "localhost" to ::1 first, so Vite's default
      // host ("localhost") ends up bound to IPv6 only. The policy-rpc
      // server's OAuth callback redirects to a hard-coded
      // `http://127.0.0.1:5173` (IPv4), which an IPv6-only listener
      // refuses. Pinning host to the IPv4 loopback keeps both
      // `127.0.0.1` and `localhost` reachable.
      host: "127.0.0.1",
      fs: {
        // Allow importing from the sibling sdk/ folder (one level up).
        allow: [".."],
      },
    },
    resolve: {
      alias: {
        "@pasu/sdk": path.resolve(__dirname, "../sdk/extension-client.ts"),
      },
    },
  };
});
