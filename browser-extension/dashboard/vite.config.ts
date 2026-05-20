import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "node:path";

// The extension's manifest content_scripts entry pins the bridge to
// http://localhost:5174 / http://127.0.0.1:5174. Keep this port matching
// or the bridge will not inject and every SDK call will time out.
export default defineConfig({
  plugins: [react()],
  server: {
    port: 5174,
    strictPort: true,
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
