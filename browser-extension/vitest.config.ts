import { defineConfig } from "vitest/config";
import path from "path";

export default defineConfig({
  test: {
    environment: "happy-dom",
    globals: true,
    // Dashboard modules resolve i18n labels through t() at call time, so the
    // i18n instance must be initialized before any test runs. The dashboard's
    // own vitest config sets this, but CI runs the single ROOT config from
    // browser-extension/ — without this, dashboard tests (nl/manifest/…) see
    // uninitialized i18n and t() returns undefined.
    setupFiles: ["dashboard/src/i18n/vitest-setup.ts"],
    coverage: { provider: "v8" },
    // On macOS+iCloud the dep dirs are `node_modules.nosync/` (with a plain
    // `node_modules` symlink) — vitest's default exclude only matches
    // `**/node_modules/**`, so without this the runner sweeps dependency
    // test files out of the .nosync tree.
    exclude: ["**/node_modules/**", "**/node_modules.nosync/**", "**/dist/**"],
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
