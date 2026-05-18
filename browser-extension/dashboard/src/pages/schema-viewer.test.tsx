// Schema-viewer tests (Phase 7.3).
//
// The page renders the currently-installed enriched cedarschema:
//   - Left rail: 34 actions from REGISTERED_ACTIONS (hardcoded mirror).
//   - Main pane: base context fields (muted) + custom context fields
//     (accent) for the selected action.
//   - Hash badge: shows `schemaHash` (camelCase from getEnrichedSchema).
//   - Raw Cedar toggle: flips main pane to a <pre> with `schema_text`.
//
// We mock the SDK at the context layer using `TestSdkProvider`. Our
// mocks model the actual `EnrichedSchemaOutput` returned by the SDK
// (snake_case `schema_text`/`schema_hash`, camelCase `customContexts`/
// `schemaHash`, snake_case inner fields on CustomFieldSource).

import { describe, expect, it, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { MemoryRouter, Routes, Route } from "react-router-dom";
import type { EnrichedSchemaOutput, ExtensionClient } from "@scopeball/sdk";
import { SchemaViewer } from "./schema-viewer";
import { TestSdkProvider } from "../testing/test-sdk-provider";

function mkEnriched(): EnrichedSchemaOutput {
  return {
    schema_text:
      "type SwapContext = {\n" +
      "  swapMode: String,\n" +
      "  inputToken: AssetRefWithAmountConstraint,\n" +
      "  outputToken: AssetRefWithAmountConstraint,\n" +
      "  recipient: String,\n" +
      "  custom?: SwapCustomContext,\n" +
      "};\n" +
      "\n" +
      "type SwapCustomContext = {\n" +
      "  totalInputUsd: UsdValuation,\n" +
      "};\n",
    schema_hash: "sha256:installed",
    added_fields: [],
    customContexts: {
      swap: [
        {
          field: "totalInputUsd",
          cedar_type: "UsdValuation",
          source_method: "oracle.usd_value",
          source_requirement_id: "req-x",
          source_from: "$.result.value",
          requirement_optional: false,
        },
      ],
    },
    schemaHash: "sha256:abc",
  };
}

function renderViewer(
  initialUrl: string,
  overrides: Partial<ExtensionClient>,
) {
  const client = {
    getEnrichedSchema: vi.fn(async () => mkEnriched()),
    ...overrides,
  } as unknown as ExtensionClient;
  const utils = render(
    <MemoryRouter initialEntries={[initialUrl]}>
      <TestSdkProvider client={client}>
        <Routes>
          <Route path="/schema" element={<SchemaViewer />} />
        </Routes>
      </TestSdkProvider>
    </MemoryRouter>,
  );
  return { client, ...utils };
}

describe("SchemaViewer", () => {
  it("renders the custom field with source-method metadata for the selected action", async () => {
    renderViewer("/schema?action=swap", {});

    // Custom field name appears in the custom section with the accent class.
    const customCell = await screen.findByText("totalInputUsd");
    expect(customCell.className).toMatch(/custom/);

    // Source method is visible (not hover-only) so happy-dom + RTL can see it.
    expect(screen.getByText(/oracle\.usd_value/)).toBeTruthy();

    // Provenance metadata is also reachable via the tooltip / title attr.
    const customRow =
      customCell.closest("[data-testid='custom-field-row']") ??
      customCell.parentElement;
    expect(customRow).toBeTruthy();
    expect(customRow?.getAttribute("title") ?? "").toContain("req-x");
  });

  it("renders base fields parsed from schema_text in muted style", async () => {
    renderViewer("/schema?action=swap", {});

    // `swapMode` is a base field (declared in SwapContext, not the custom
    // bridge). Filter ensures `custom` itself isn't rendered as a base
    // field row.
    const baseCell = await screen.findByText("swapMode");
    expect(baseCell.className).toMatch(/base/);
    expect(screen.queryByText(/^custom$/)).toBeNull();
  });

  it("shows the schemaHash (camelCase) in the hash badge", async () => {
    renderViewer("/schema?action=swap", {});

    await waitFor(() =>
      expect(screen.getByTestId("schema-hash-badge").textContent).toContain(
        "sha256:abc",
      ),
    );
  });

  it("lists every registered action in the left rail (34 entries)", async () => {
    renderViewer("/schema?action=swap", {});
    // Defer to the rail container to filter out chrome content.
    const rail = await screen.findByRole("navigation", {
      name: /actions/i,
    });
    const links = rail.querySelectorAll("a, button");
    expect(links.length).toBe(34);
  });

  it("toggles to raw Cedar view when the toggle button is pressed", async () => {
    renderViewer("/schema?action=swap", {});

    await screen.findByText("totalInputUsd");
    const toggle = screen.getByRole("button", { name: /raw cedar/i });
    fireEvent.click(toggle);

    // After toggle, the raw schema text is rendered in a <pre>.
    const pre = await screen.findByTestId("schema-raw-pre");
    expect(pre.textContent).toContain("type SwapContext = {");
    expect(pre.tagName.toLowerCase()).toBe("pre");
  });

  it("falls back to action=swap when no query param is provided", async () => {
    renderViewer("/schema", {});
    await screen.findByText("totalInputUsd");
    // The selected rail entry is `swap`.
    const railSwap = screen.getByRole("link", { name: /^swap$/i });
    expect(railSwap.className).toMatch(/selected|active/);
  });
});
