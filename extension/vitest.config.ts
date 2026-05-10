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
      "@lib": path.resolve(__dirname, "src/lib"),
      "@background": path.resolve(__dirname, "src/background"),
    },
  },
});
