// Onboarding wizard tests (Phase 7.4).
//
// Three-step wizard:
//   1. Intro — two-sentence explanation of the per-RPC-endpoint model.
//   2. Endpoint URL prompt — same scheme validation as `/rpc-endpoint`,
//      plus a "Use local dev server" shortcut that is only visible when
//      `process.env.NODE_ENV !== "production"`.
//   3. Import manifests — paste-JSON map or "Skip" to land on `/policies`.
//
// Manifests imported in step 3 are POSTed one at a time via
// SDK.putManifest. Per-action errors are surfaced inline so a bad entry
// in the middle of a paste doesn't silently swallow the rest.

import { describe, expect, it, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { MemoryRouter, Routes, Route } from "react-router-dom";
import type {
  ExtensionClient,
  ManifestPutResult,
  PingResult,
} from "@scopeball/sdk";
import { OnboardingPage } from "./onboarding";
import { TestSdkProvider } from "../testing/test-sdk-provider";

function noEndpoint(): PingResult {
  return { reachable: false, url: null };
}

function mkPutResult(): ManifestPutResult {
  return { enrichedSchemaHash: "sha256:after", addedCustomFields: {} };
}

function renderWizard(overrides: Partial<ExtensionClient>) {
  const client = {
    pingRpcEndpoint: vi.fn(async () => noEndpoint()),
    setEndpointUrl: vi.fn(async (url: string | null) => ({ url })),
    putManifest: vi.fn(async () => mkPutResult()),
    ...overrides,
  } as unknown as ExtensionClient;
  const utils = render(
    <MemoryRouter initialEntries={["/onboarding"]}>
      <TestSdkProvider client={client}>
        <Routes>
          <Route path="/onboarding" element={<OnboardingPage />} />
          <Route path="/policies" element={<div>policies-route</div>} />
        </Routes>
      </TestSdkProvider>
    </MemoryRouter>,
  );
  return { client, ...utils };
}

describe("OnboardingPage", () => {
  it("starts on the intro step and advances through the wizard", async () => {
    renderWizard({});
    await screen.findByText(/Set your policy-rpc endpoint/i);
    fireEvent.click(screen.getByRole("button", { name: /^Next$/ }));
    // Step 2: URL prompt.
    expect(screen.getByLabelText(/Endpoint URL/i)).toBeTruthy();
  });

  it("step 2 saves the endpoint URL and advances to import", async () => {
    const setEndpointUrl = vi.fn(async (url: string | null) => ({ url }));
    renderWizard({ setEndpointUrl });
    // Step 1 → Step 2.
    fireEvent.click(screen.getByRole("button", { name: /^Next$/ }));
    const input = screen.getByLabelText(/Endpoint URL/i);
    fireEvent.change(input, { target: { value: "http://localhost:8787" } });
    fireEvent.click(screen.getByRole("button", { name: /^Next$/ }));
    await waitFor(() =>
      expect(setEndpointUrl).toHaveBeenCalledWith("http://localhost:8787"),
    );
    // Step 3: import.
    await screen.findByText(/Import manifests/i);
  });

  it("step 2 rejects file:// scheme client-side", async () => {
    const setEndpointUrl = vi.fn(async (url: string | null) => ({ url }));
    renderWizard({ setEndpointUrl });
    fireEvent.click(screen.getByRole("button", { name: /^Next$/ }));
    const input = screen.getByLabelText(/Endpoint URL/i);
    fireEvent.change(input, { target: { value: "file:///bad" } });
    fireEvent.click(screen.getByRole("button", { name: /^Next$/ }));
    expect(screen.getByText(/http:\/\/ or https:\/\/ only/i)).toBeTruthy();
    expect(setEndpointUrl).not.toHaveBeenCalled();
  });

  it("step 2 shows the localhost shortcut in dev builds", () => {
    // happy-dom doesn't set NODE_ENV='production'; in this test runner
    // we're in 'test', so the shortcut should be visible.
    renderWizard({});
    fireEvent.click(screen.getByRole("button", { name: /^Next$/ }));
    expect(
      screen.getByRole("button", { name: /Use local dev server/i }),
    ).toBeTruthy();
  });

  it("step 3: import succeeds for each action and lands on /policies", async () => {
    const putManifest = vi.fn(async () => mkPutResult());
    renderWizard({ putManifest });
    // Walk through steps 1 + 2.
    fireEvent.click(screen.getByRole("button", { name: /^Next$/ }));
    fireEvent.change(screen.getByLabelText(/Endpoint URL/i), {
      target: { value: "http://localhost:8787" },
    });
    fireEvent.click(screen.getByRole("button", { name: /^Next$/ }));

    await screen.findByText(/Import manifests/i);
    const paste = screen.getByLabelText(/Paste manifests JSON/i);
    const map = {
      swap: { id: "user.swap.v1", schema_version: 1, requires: [] },
      transfer: {
        id: "user.transfer.v1",
        schema_version: 1,
        requires: [],
      },
    };
    fireEvent.change(paste, { target: { value: JSON.stringify(map) } });
    fireEvent.click(screen.getByRole("button", { name: /^Import$/ }));
    await waitFor(() => expect(putManifest).toHaveBeenCalledTimes(2));
    expect(putManifest).toHaveBeenCalledWith(
      "swap",
      expect.objectContaining({ id: "user.swap.v1" }),
    );
    expect(putManifest).toHaveBeenCalledWith(
      "transfer",
      expect.objectContaining({ id: "user.transfer.v1" }),
    );
    await screen.findByText("policies-route");
  });

  it("step 3: invalid JSON surfaces an error without calling putManifest", async () => {
    const putManifest = vi.fn(async () => mkPutResult());
    renderWizard({ putManifest });
    fireEvent.click(screen.getByRole("button", { name: /^Next$/ }));
    fireEvent.change(screen.getByLabelText(/Endpoint URL/i), {
      target: { value: "http://localhost:8787" },
    });
    fireEvent.click(screen.getByRole("button", { name: /^Next$/ }));
    await screen.findByText(/Import manifests/i);

    const paste = screen.getByLabelText(/Paste manifests JSON/i);
    fireEvent.change(paste, { target: { value: "not-json" } });
    fireEvent.click(screen.getByRole("button", { name: /^Import$/ }));
    await screen.findByText(/invalid JSON/i);
    expect(putManifest).not.toHaveBeenCalled();
  });

  it("step 3: per-action error is surfaced and other actions still install", async () => {
    const putManifest = vi.fn(async (action: string) => {
      if (action === "broken") {
        throw Object.assign(new Error("bad manifest"), {
          kind: "schema_invalid",
          message: "bad manifest",
        });
      }
      return mkPutResult();
    });
    renderWizard({ putManifest });
    fireEvent.click(screen.getByRole("button", { name: /^Next$/ }));
    fireEvent.change(screen.getByLabelText(/Endpoint URL/i), {
      target: { value: "http://localhost:8787" },
    });
    fireEvent.click(screen.getByRole("button", { name: /^Next$/ }));
    await screen.findByText(/Import manifests/i);

    const map = {
      swap: { id: "user.swap.v1", schema_version: 1, requires: [] },
      broken: { id: "user.broken.v1", schema_version: 1, requires: [] },
    };
    fireEvent.change(screen.getByLabelText(/Paste manifests JSON/i), {
      target: { value: JSON.stringify(map) },
    });
    fireEvent.click(screen.getByRole("button", { name: /^Import$/ }));

    await screen.findByText(/schema_invalid/i);
    // The error row mentions the offending action so the user knows
    // which entry to fix. The textarea also contains the literal
    // "broken" key, so we scope to the error list explicitly.
    const errorList = await screen.findByRole("list");
    expect(errorList.textContent).toMatch(/broken/);
    // The good entry still went through.
    expect(putManifest).toHaveBeenCalledWith(
      "swap",
      expect.objectContaining({ id: "user.swap.v1" }),
    );
  });

  it("step 3: Skip navigates to /policies without calling putManifest", async () => {
    const putManifest = vi.fn(async () => mkPutResult());
    renderWizard({ putManifest });
    fireEvent.click(screen.getByRole("button", { name: /^Next$/ }));
    fireEvent.change(screen.getByLabelText(/Endpoint URL/i), {
      target: { value: "http://localhost:8787" },
    });
    fireEvent.click(screen.getByRole("button", { name: /^Next$/ }));
    await screen.findByText(/Import manifests/i);

    fireEvent.click(screen.getByRole("button", { name: /^Skip$/ }));
    await screen.findByText("policies-route");
    expect(putManifest).not.toHaveBeenCalled();
  });
});
