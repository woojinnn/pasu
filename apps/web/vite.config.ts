import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "node:path";

// `apps/web` dev server defaults to localhost:5175 — distinct from
// browser-extension/dashboard (5173) and any preview servers (5174).
// The policy-rpc server defaults to 127.0.0.1:8788; CORS is permissive.
export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      "@scopeball/api-client": path.resolve(
        __dirname,
        "../../packages/api-client/src/index.ts",
      ),
      "@scopeball/types": path.resolve(
        __dirname,
        "../../packages/types/src/index.ts",
      ),
    },
  },
  server: {
    port: 5175,
    strictPort: true,
  },
});
