// Manifest editor → schema viewer Preview hand-off (Phase 7 carry-over J).
//
// The editor's "Preview" button used to navigate to `/schema?action=…`
// and drop the previewed manifest output on the floor — the viewer
// re-fetched the currently-installed schema and a "diff overlay vs
// draft manifests is a Phase-7 follow-up" placeholder was shown.
//
// The new flow stashes `PreviewManifestOutput` in `sessionStorage`
// before navigating; SchemaViewer reads (and clears) that slot on
// mount, renders a "Draft preview from your unsaved manifest." banner,
// and overlays the previewed custom fields on top of the installed
// schema.

import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { MemoryRouter, Routes, Route } from "react-router-dom";
import type {
  EnrichedSchemaOutput,
  ExtensionClient,
  PreviewManifestOutput,
} from "@scopeball/sdk";
import { ManifestEditor, PREVIEW_HANDOFF_KEY } from "./manifest-editor";
import { SchemaViewer } from "./schema-viewer";
import { TestSdkProvider } from "../testing/test-sdk-provider";

function previewWithTotalInputUsd(): PreviewManifestOutput {
  return {
    customTypes: [
      {
        name: "swap",
        fields: [
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
    ],
    enrichedSchemaText:
      "type SwapContext = {\n  custom?: SwapCustomContext,\n};\n" +
      "type SwapCustomContext = {\n  totalInputUsd: UsdValuation,\n};\n",
    diff: { added: [], removed: [], changed: [] },
    schemaHash: "sha256:draft-preview-hash",
  };
}

function installedSchemaEmpty(): EnrichedSchemaOutput {
  return {
    schema_text: "type SwapContext = {};",
    schema_hash: "sha256:installed",
    added_fields: [],
    customContexts: {},
    schemaHash: "sha256:installed",
  };
}

function mkClient(overrides: Partial<ExtensionClient>): ExtensionClient {
  return {
    getAliasTable: vi.fn(async () => ({ entries: [] })),
    getManifest: vi.fn(async () => ({ manifest: null })),
    previewManifest: vi.fn(async () => previewWithTotalInputUsd()),
    getEnrichedSchema: vi.fn(async () => installedSchemaEmpty()),
    ...overrides,
  } as unknown as ExtensionClient;
}

function renderFullFlow(client: ExtensionClient) {
  return render(
    <MemoryRouter initialEntries={["/manifests/swap"]}>
      <TestSdkProvider client={client}>
        <Routes>
          <Route path="/manifests/:action" element={<ManifestEditor />} />
          <Route path="/schema" element={<SchemaViewer />} />
        </Routes>
      </TestSdkProvider>
    </MemoryRouter>,
  );
}

describe("Manifest editor → Schema viewer Preview hand-off", () => {
  beforeEach(() => {
    try {
      sessionStorage.clear();
    } catch {
      /* happy-dom safety */
    }
  });

  it("stashes the preview output and navigates with fromPreview=true", async () => {
    const client = mkClient({});
    renderFullFlow(client);

    fireEvent.change(await screen.findByLabelText(/manifest id/i), {
      target: { value: "user.swap.v1" },
    });
    fireEvent.click(screen.getByText(/^Preview$/));

    // After navigation the viewer mounts and consumes the hand-off.
    // Banner + draft pill must be visible.
    await screen.findByTestId("schema-from-preview-banner");
    expect(screen.getByTestId("schema-viewer-draft-pill")).toBeTruthy();

    // The previewed custom field shows up in the custom section even
    // though the installed `customContexts` is empty.
    await screen.findByText("totalInputUsd");

    // Hash badge reflects the previewed `schemaHash`.
    expect(screen.getByTestId("schema-hash-badge").textContent).toContain(
      "sha256:draft-preview-hash",
    );
  });

  it("clears the sessionStorage slot after the viewer consumes it", async () => {
    const client = mkClient({});
    renderFullFlow(client);
    fireEvent.click(await screen.findByText(/^Preview$/));
    await screen.findByTestId("schema-from-preview-banner");

    expect(sessionStorage.getItem(PREVIEW_HANDOFF_KEY)).toBeNull();
  });

  it("falls back to the installed schema when the hand-off action mismatches", async () => {
    const client = mkClient({});
    // Pre-seed a stale hand-off for a different action.
    sessionStorage.setItem(
      PREVIEW_HANDOFF_KEY,
      JSON.stringify({
        action: "stake",
        output: previewWithTotalInputUsd(),
        savedAtMs: Date.now(),
      }),
    );

    render(
      <MemoryRouter initialEntries={["/schema?action=swap&fromPreview=true"]}>
        <TestSdkProvider client={client}>
          <Routes>
            <Route path="/schema" element={<SchemaViewer />} />
          </Routes>
        </TestSdkProvider>
      </MemoryRouter>,
    );

    // Mismatched hand-off → viewer renders the "no unsaved draft found"
    // variant of the from-preview banner.
    await screen.findByText(/No unsaved draft was found/i);
    expect(screen.queryByTestId("schema-viewer-draft-pill")).toBeNull();

    // Slot was still cleared so a later visit doesn't pick it back up.
    expect(sessionStorage.getItem(PREVIEW_HANDOFF_KEY)).toBeNull();
  });
});
