// RPC endpoint settings page tests (Phase 7.4).
//
// The page exposes:
//   - An "Endpoint URL" text input pre-seeded from the SW (we use
//     `pingRpcEndpoint()`'s `url` field — see `rpc-endpoint.tsx` for why
//     we don't have a dedicated `getEndpointUrl` SDK method).
//   - A "Ping" button that calls SDK.pingRpcEndpoint and surfaces the
//     `{reachable, status, message}` result inline.
//   - A "Save" button that validates the scheme client-side ("http://"
//     or "https://" only) and then calls SDK.setEndpointUrl.
//   - A "Clear all manifests" button — disabled for v1 (no SW handler
//     yet; tracked in the commit message).
//
// We mock the SDK via `TestSdkProvider`.

import { describe, expect, it, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { MemoryRouter, Routes, Route } from "react-router-dom";
import type { ExtensionClient, PingResult } from "@scopeball/sdk";
import { RpcEndpointPage } from "./rpc-endpoint";
import { TestSdkProvider } from "../testing/test-sdk-provider";

function noEndpoint(): PingResult {
  return { reachable: false, url: null };
}

function reachable(url: string): PingResult {
  return { reachable: true, url, status: 200 };
}

function unreachable(url: string, message: string): PingResult {
  return { reachable: false, url, message };
}

function renderPage(overrides: Partial<ExtensionClient>) {
  const client = {
    pingRpcEndpoint: vi.fn(async () => noEndpoint()),
    setEndpointUrl: vi.fn(async (url: string | null) => ({ url })),
    ...overrides,
  } as unknown as ExtensionClient;
  const utils = render(
    <MemoryRouter initialEntries={["/rpc-endpoint"]}>
      <TestSdkProvider client={client}>
        <Routes>
          <Route path="/rpc-endpoint" element={<RpcEndpointPage />} />
        </Routes>
      </TestSdkProvider>
    </MemoryRouter>,
  );
  return { client, ...utils };
}

describe("RpcEndpointPage", () => {
  it("seeds the URL input from pingRpcEndpoint().url on mount", async () => {
    const pingRpcEndpoint = vi.fn(async () =>
      reachable("http://localhost:8787"),
    );
    renderPage({ pingRpcEndpoint });
    await waitFor(() => expect(pingRpcEndpoint).toHaveBeenCalled());
    const input = (await screen.findByLabelText(
      /Endpoint URL/i,
    )) as HTMLInputElement;
    await waitFor(() => expect(input.value).toBe("http://localhost:8787"));
  });

  it("rejects file:// URLs with 'http:// or https:// only' before calling setEndpointUrl", async () => {
    const setEndpointUrl = vi.fn(async (url: string | null) => ({ url }));
    renderPage({ setEndpointUrl });
    const input = await screen.findByLabelText(/Endpoint URL/i);
    fireEvent.change(input, { target: { value: "file:///etc/passwd" } });
    fireEvent.click(screen.getByRole("button", { name: /^Save$/ }));
    expect(
      screen.getByText(/http:\/\/ or https:\/\/ only/i),
    ).toBeTruthy();
    expect(setEndpointUrl).not.toHaveBeenCalled();
  });

  it("accepts http:// and persists via setEndpointUrl", async () => {
    const setEndpointUrl = vi.fn(async (url: string | null) => ({ url }));
    renderPage({ setEndpointUrl });
    const input = await screen.findByLabelText(/Endpoint URL/i);
    fireEvent.change(input, { target: { value: "http://localhost:8787" } });
    fireEvent.click(screen.getByRole("button", { name: /^Save$/ }));
    await waitFor(() =>
      expect(setEndpointUrl).toHaveBeenCalledWith("http://localhost:8787"),
    );
  });

  it("Ping shows the reachable status with HTTP code", async () => {
    const pingRpcEndpoint = vi
      .fn(async (): Promise<PingResult> => noEndpoint())
      // First call: mount-time seed (no URL configured).
      .mockResolvedValueOnce(noEndpoint())
      // Second call: explicit Ping click.
      .mockResolvedValueOnce(reachable("http://localhost:8787"));
    renderPage({ pingRpcEndpoint });
    await waitFor(() => expect(pingRpcEndpoint).toHaveBeenCalledTimes(1));

    fireEvent.click(screen.getByRole("button", { name: /^Ping$/ }));
    await screen.findByText(/reachable/i);
    expect(screen.getByText(/200/)).toBeTruthy();
  });

  it("Ping surfaces a connection failure message", async () => {
    const pingRpcEndpoint = vi
      .fn(async (): Promise<PingResult> => noEndpoint())
      .mockResolvedValueOnce(noEndpoint())
      .mockResolvedValueOnce(
        unreachable("http://localhost:8787", "ECONNREFUSED"),
      );
    renderPage({ pingRpcEndpoint });
    await waitFor(() => expect(pingRpcEndpoint).toHaveBeenCalledTimes(1));

    fireEvent.click(screen.getByRole("button", { name: /^Ping$/ }));
    await screen.findByText(/ECONNREFUSED/);
  });

  it("Clear all manifests is disabled (deferred to v2)", async () => {
    renderPage({});
    const btn = await screen.findByRole("button", {
      name: /Clear all manifests/i,
    });
    expect((btn as HTMLButtonElement).disabled).toBe(true);
  });
});
