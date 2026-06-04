import { defineConfig } from "vite";
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
export default defineConfig({
  plugins: [react()],
  base: "./",
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
      "@scopeball/sdk": path.resolve(__dirname, "../sdk/extension-client.ts"),
    },
  },
});
