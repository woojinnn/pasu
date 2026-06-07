// @vitest-environment node

import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";

import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { resolveServerUrlEnv } from "../vite.config";

describe("dashboard Vite server URL env", () => {
  let savedPasuServerUrl: string | undefined;
  let savedVitePasuServerUrl: string | undefined;

  beforeEach(() => {
    savedPasuServerUrl = process.env.PASU_SERVER_URL;
    savedVitePasuServerUrl = process.env.VITE_PASU_SERVER_URL;
    delete process.env.PASU_SERVER_URL;
    delete process.env.VITE_PASU_SERVER_URL;
  });

  afterEach(() => {
    if (savedPasuServerUrl === undefined) delete process.env.PASU_SERVER_URL;
    else process.env.PASU_SERVER_URL = savedPasuServerUrl;
    if (savedVitePasuServerUrl === undefined) delete process.env.VITE_PASU_SERVER_URL;
    else process.env.VITE_PASU_SERVER_URL = savedVitePasuServerUrl;
  });

  it("reads PASU_SERVER_URL from the extension root when build:options runs in the dashboard workspace", () => {
    const root = mkdtempSync(path.join(tmpdir(), "pasu-extension-env-"));
    const dashboardDir = path.join(root, "dashboard");
    mkdirSync(dashboardDir);
    writeFileSync(
      path.join(root, ".env"),
      "PASU_SERVER_URL=https://pasu-policy.example.test\n",
    );

    expect(resolveServerUrlEnv("production", dashboardDir)).toBe(
      "https://pasu-policy.example.test",
    );
  });

  it("lets an explicit dashboard env override the extension root default", () => {
    const root = mkdtempSync(path.join(tmpdir(), "pasu-extension-env-"));
    const dashboardDir = path.join(root, "dashboard");
    mkdirSync(dashboardDir);
    writeFileSync(path.join(root, ".env"), "PASU_SERVER_URL=https://root.example.test\n");
    writeFileSync(
      path.join(dashboardDir, ".env"),
      "PASU_SERVER_URL=https://dashboard.example.test\n",
    );

    expect(resolveServerUrlEnv("production", dashboardDir)).toBe(
      "https://dashboard.example.test",
    );
  });

  it("keeps development builds local even when the extension root .env is for production", () => {
    const root = mkdtempSync(path.join(tmpdir(), "pasu-extension-env-"));
    const dashboardDir = path.join(root, "dashboard");
    mkdirSync(dashboardDir);
    writeFileSync(path.join(root, ".env"), "PASU_SERVER_URL=https://root.example.test\n");

    expect(resolveServerUrlEnv("development", dashboardDir)).toBe("");
  });

  it("still lets an explicit development env point at another server", () => {
    const root = mkdtempSync(path.join(tmpdir(), "pasu-extension-env-"));
    const dashboardDir = path.join(root, "dashboard");
    mkdirSync(dashboardDir);
    writeFileSync(path.join(root, ".env"), "PASU_SERVER_URL=https://root.example.test\n");
    process.env.PASU_SERVER_URL = "https://dev-override.example.test";

    expect(resolveServerUrlEnv("development", dashboardDir)).toBe(
      "https://dev-override.example.test",
    );
  });
});
