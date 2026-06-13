import { createRequire } from "node:module";

import { describe, expect, it } from "vitest";

const require = createRequire(import.meta.url);

describe("webpack env mode helpers", () => {
  it("does not load the production .env file for development builds", () => {
    const { envFileNameForMode } = require("../../../webpack/env.js") as {
      envFileNameForMode(mode: string): string;
    };

    expect(envFileNameForMode("development")).toBe(".env.development");
    expect(envFileNameForMode("production")).toBe(".env");
  });

  it("uses the deployed policy server URL unless explicitly overridden", () => {
    const { resolveServerUrl } = require("../../../webpack/env.js") as {
      resolveServerUrl(env: Record<string, string | undefined>): string;
    };

    expect(resolveServerUrl({})).toBe("https://dambi-policy.duckdns.org");
    expect(
      resolveServerUrl({ DAMBI_SERVER_URL: "https://dambi-policy.example.test" }),
    ).toBe("https://dambi-policy.example.test");
  });
});
