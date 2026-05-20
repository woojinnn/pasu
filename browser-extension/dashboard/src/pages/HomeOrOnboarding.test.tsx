// Tests for the cold-start onboarding redirect (Phase 7 carry-over I).
//
// Spec D11: a brand-new install lands on `/onboarding` instead of
// `/` (HomePage) — both `rpc:endpointUrl` and `rpc:manifests` are empty
// in storage, so HomePage's surfaces would point at nothing useful.

import { describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter, Routes, Route } from "react-router-dom";
import type {
  EnrichedSchemaOutput,
  ExtensionClient,
  PingResult,
} from "@scopeball/sdk";
import { HomeOrOnboarding } from "./HomeOrOnboarding";
import { TestSdkProvider } from "../testing/test-sdk-provider";

function emptySchema(): EnrichedSchemaOutput {
  return {
    schema_text: "",
    schema_hash: "sha256:empty",
    added_fields: [],
    customContexts: {},
    schemaHash: "sha256:empty",
  };
}

function schemaWithSwapField(): EnrichedSchemaOutput {
  return {
    schema_text: "",
    schema_hash: "sha256:nonempty",
    added_fields: [],
    customContexts: {
      swap: [
        {
          field: "totalInputUsd",
          cedar_type: "UsdValuation",
          source_method: "oracle.usd_value",
          source_requirement_id: "swap-total-input-usd",
          source_from: "$.result",
          requirement_optional: true,
        },
      ],
    },
    schemaHash: "sha256:nonempty",
  };
}

function renderHomeOrOnboarding(client: Partial<ExtensionClient>) {
  const fullClient = client as ExtensionClient;
  return render(
    <MemoryRouter initialEntries={["/"]}>
      <TestSdkProvider client={fullClient}>
        <Routes>
          <Route path="/" element={<HomeOrOnboarding />} />
          <Route path="/onboarding" element={<div>onboarding-route</div>} />
        </Routes>
      </TestSdkProvider>
    </MemoryRouter>,
  );
}

describe("HomeOrOnboarding redirect", () => {
  it("redirects to /onboarding when endpoint URL is null AND no manifests", async () => {
    const client = {
      pingRpcEndpoint: vi.fn(
        async (): Promise<PingResult> => ({ reachable: false, url: null }),
      ),
      getEnrichedSchema: vi.fn(async () => emptySchema()),
    };
    renderHomeOrOnboarding(client);
    await waitFor(() =>
      expect(screen.getByText("onboarding-route")).toBeTruthy(),
    );
    expect(client.pingRpcEndpoint).toHaveBeenCalledTimes(1);
    expect(client.getEnrichedSchema).toHaveBeenCalledTimes(1);
  });

  it("renders HomePage when endpoint URL is set, even if manifests are empty", async () => {
    const client = {
      pingRpcEndpoint: vi.fn(
        async (): Promise<PingResult> => ({
          reachable: true,
          url: "http://localhost:8787",
        }),
      ),
      getEnrichedSchema: vi.fn(async () => emptySchema()),
    };
    renderHomeOrOnboarding(client);
    // HomePage renders the hero heading.
    await waitFor(() =>
      expect(screen.queryByText("onboarding-route")).toBeNull(),
    );
    // Some HomePage marker — the hero <h1> Korean string.
    await screen.findByRole("heading", { level: 1 });
  });

  it("renders HomePage when manifests exist, even if endpoint URL is null", async () => {
    const client = {
      pingRpcEndpoint: vi.fn(
        async (): Promise<PingResult> => ({ reachable: false, url: null }),
      ),
      getEnrichedSchema: vi.fn(async () => schemaWithSwapField()),
    };
    renderHomeOrOnboarding(client);
    await waitFor(() =>
      expect(screen.queryByText("onboarding-route")).toBeNull(),
    );
    await screen.findByRole("heading", { level: 1 });
  });

  it("falls back to HomePage on SDK read failure", async () => {
    const client = {
      pingRpcEndpoint: vi.fn(async () => {
        throw new Error("offline");
      }),
      getEnrichedSchema: vi.fn(async () => emptySchema()),
    };
    renderHomeOrOnboarding(client);
    await waitFor(() =>
      expect(screen.queryByText("onboarding-route")).toBeNull(),
    );
    await screen.findByRole("heading", { level: 1 });
  });
});
