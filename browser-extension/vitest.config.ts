import { defineConfig } from "vitest/config";
import path from "path";

export default defineConfig({
  test: {
    environment: "happy-dom",
    globals: true,
    coverage: { provider: "v8" },
  },
  resolve: {
    alias: {
      "@lib": path.resolve(__dirname, "backend/lib"),
      "@background": path.resolve(__dirname, "backend/service-worker"),
      // Dashboard tests resolve `@pasu/sdk` the same way the dashboard
      // Vite build does (see `dashboard/vite.config.ts`). Keeping the
      // alias here lets `dashboard/**/*.test.tsx` files run under the
      // single root vitest config.
      "@pasu/sdk": path.resolve(__dirname, "sdk/extension-client.ts"),
    },
  },
});
