// Manifest-editor tests (Phase 7.2).
//
// The editor is a form for one action's `PolicyManifest`. It owns:
//   - the top-level `id` field
//   - a list of `requires[]` rows (id, method, optional, params k/v rows)
//   - a list of `outputs[]` rows (field, type, from, required)
//
// Two buttons:
//   - Preview → SDK.previewManifest, then navigate to `/schema?action=…`
//   - Save    → SDK.putManifest, on thrown error envelope show message
//
// We mock the SDK at the context layer using a stand-in ExtensionProvider
// that hands the test-provided client to `useExtension()`.

import { describe, expect, it, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { MemoryRouter, Routes, Route } from "react-router-dom";
import type {
  ExtensionClient,
  PreviewManifestOutput,
  ManifestPutResult,
} from "@scopeball/sdk";
import { ManifestEditor } from "./manifest-editor";
import { TestSdkProvider } from "../testing/test-sdk-provider";

function mkPreviewResult(): PreviewManifestOutput {
  return {
    customTypes: [{ name: "SwapCustomContext", fields: [] }],
    enrichedSchemaText: "type SwapCustomContext = {};",
    diff: { added: [], removed: [], changed: [] },
    schemaHash: "sha256:test",
  };
}

function mkPutResult(): ManifestPutResult {
  return { enrichedSchemaHash: "sha256:after-install", addedCustomFields: {} };
}

function fakeAliasTable() {
  return {
    entries: [
      { name: "String", kind: "scalar" as const, cedarSpelling: "String" },
      { name: "Long", kind: "scalar" as const, cedarSpelling: "Long" },
      {
        name: "UsdValuation",
        kind: "record" as const,
        cedarSpelling: "UsdValuation",
      },
    ],
  };
}

function renderEditor(
  action: string,
  overrides: Partial<ExtensionClient>,
) {
  const client = {
    previewManifest: vi.fn(async () => mkPreviewResult()),
    putManifest: vi.fn(async () => mkPutResult()),
    getManifest: vi.fn(async () => ({ manifest: null })),
    getAliasTable: vi.fn(async () => fakeAliasTable()),
    ...overrides,
  } as unknown as ExtensionClient;
  const utils = render(
    <MemoryRouter initialEntries={[`/manifests/${action}`]}>
      <TestSdkProvider client={client}>
        <Routes>
          <Route path="/manifests/:action" element={<ManifestEditor />} />
          <Route path="/schema" element={<div>schema-route</div>} />
        </Routes>
      </TestSdkProvider>
    </MemoryRouter>,
  );
  return { client, ...utils };
}

describe("ManifestEditor", () => {
  it("renders the action header and an editable manifest id field", async () => {
    const { client } = renderEditor("swap", {});
    await waitFor(() => expect(client.getAliasTable).toHaveBeenCalled());
    // Loading the alias table happens on mount.
    expect(screen.getByText(/swap/i)).toBeTruthy();
    const idInput = screen.getByLabelText(/manifest id/i) as HTMLInputElement;
    expect(idInput).toBeTruthy();
  });

  it("Preview button calls previewManifest with the current form state", async () => {
    const previewManifest = vi.fn(async () => mkPreviewResult());
    const { client } = renderEditor("swap", { previewManifest });
    await waitFor(() => expect(client.getAliasTable).toHaveBeenCalled());

    const idInput = screen.getByLabelText(/manifest id/i);
    fireEvent.change(idInput, { target: { value: "user.swap.v1" } });

    fireEvent.click(screen.getByText(/^Preview$/));

    await waitFor(() =>
      expect(previewManifest).toHaveBeenCalledWith(
        "swap",
        expect.objectContaining({
          id: "user.swap.v1",
          schema_version: 1,
          requires: expect.any(Array),
        }),
      ),
    );
  });

  it("Save button calls putManifest (atomic install path)", async () => {
    const putManifest = vi.fn(async () => mkPutResult());
    const { client } = renderEditor("swap", { putManifest });
    await waitFor(() => expect(client.getAliasTable).toHaveBeenCalled());

    fireEvent.change(screen.getByLabelText(/manifest id/i), {
      target: { value: "user.swap.v1" },
    });

    fireEvent.click(screen.getByText(/^Save$/));
    await waitFor(() =>
      expect(putManifest).toHaveBeenCalledWith(
        "swap",
        expect.objectContaining({ id: "user.swap.v1" }),
      ),
    );
  });

  it("surfaces the error kind + message when Save is rejected", async () => {
    const putManifest = vi.fn(async () => {
      throw Object.assign(
        new Error(
          "outputs[0].field 'usdValue' already declared by dashboard::my-other-policy",
        ),
        {
          kind: "duplicate_field",
          message:
            "outputs[0].field 'usdValue' already declared by dashboard::my-other-policy",
        },
      );
    });
    const { client } = renderEditor("swap", { putManifest });
    await waitFor(() => expect(client.getAliasTable).toHaveBeenCalled());

    fireEvent.change(screen.getByLabelText(/manifest id/i), {
      target: { value: "user.swap.v1" },
    });
    fireEvent.click(screen.getByText(/^Save$/));

    await screen.findByText(/duplicate_field/i);
    // The offending policy id is highlighted via a <mark> with the
    // `policy-id-highlight` class — not just present in the DOM.
    const highlighted = await screen.findByText(/dashboard::my-other-policy/);
    expect(highlighted.tagName.toLowerCase()).toBe("mark");
    expect(highlighted.className).toContain("policy-id-highlight");
  });

  it("adds a new requires row when 'Add requirement' is clicked", async () => {
    const { client } = renderEditor("swap", {});
    await waitFor(() => expect(client.getAliasTable).toHaveBeenCalled());

    const before = screen.queryAllByLabelText(/requirement id/i);
    fireEvent.click(screen.getByText(/Add requirement/i));
    const after = screen.queryAllByLabelText(/requirement id/i);
    expect(after.length).toBe(before.length + 1);
  });

  it("adds an output row inside a requirement", async () => {
    const { client } = renderEditor("swap", {});
    await waitFor(() => expect(client.getAliasTable).toHaveBeenCalled());

    // Need at least one requirement to attach an output to.
    fireEvent.click(screen.getByText(/Add requirement/i));
    const before = screen.queryAllByLabelText(/output field name/i);
    fireEvent.click(screen.getByText(/Add output/i));
    const after = screen.queryAllByLabelText(/output field name/i);
    expect(after.length).toBe(before.length + 1);
  });

  // Phase 7 codex carry-over L: the Save button used to be enabled
  // whenever `busy === null`, including on a wholly empty form. Now
  // it stays disabled until the draft passes basic validation: id
  // non-empty, every requirement has an id + method, every required
  // (non-optional) requirement contributes at least one output.
  describe("Save validity gate (carry-over L)", () => {
    it("disables Save and shows a hint when the manifest id is empty", async () => {
      const putManifest = vi.fn(async () => mkPutResult());
      const { client } = renderEditor("swap", { putManifest });
      await waitFor(() => expect(client.getAliasTable).toHaveBeenCalled());

      const save = screen.getByRole("button", { name: /^Save$/ });
      expect(save.getAttribute("aria-disabled")).toBe("true");
      expect(save.hasAttribute("disabled")).toBe(true);
      expect(screen.getByTestId("manifest-validation-hint")).toBeTruthy();

      // Clicking the disabled Save must NOT call the SDK.
      fireEvent.click(save);
      expect(putManifest).not.toHaveBeenCalled();
    });

    it("enables Save once the id is filled (no requires is valid)", async () => {
      const putManifest = vi.fn(async () => mkPutResult());
      const { client } = renderEditor("swap", { putManifest });
      await waitFor(() => expect(client.getAliasTable).toHaveBeenCalled());

      fireEvent.change(screen.getByLabelText(/manifest id/i), {
        target: { value: "user.swap.v1" },
      });

      const save = screen.getByRole("button", { name: /^Save$/ });
      expect(save.hasAttribute("disabled")).toBe(false);
      expect(screen.queryByTestId("manifest-validation-hint")).toBeNull();

      fireEvent.click(save);
      await waitFor(() => expect(putManifest).toHaveBeenCalledTimes(1));
    });

    it("keeps Save disabled when a non-optional requirement has no outputs", async () => {
      const putManifest = vi.fn(async () => mkPutResult());
      const { client } = renderEditor("swap", { putManifest });
      await waitFor(() => expect(client.getAliasTable).toHaveBeenCalled());

      // Fill the manifest id; add a required requirement WITHOUT outputs.
      fireEvent.change(screen.getByLabelText(/manifest id/i), {
        target: { value: "user.swap.v1" },
      });
      fireEvent.click(screen.getByText(/Add requirement/i));
      fireEvent.change(screen.getByLabelText(/requirement id/i), {
        target: { value: "oracle-usd" },
      });
      fireEvent.change(screen.getByLabelText(/requirement method/i), {
        target: { value: "oracle.usd_value" },
      });

      // Required requirement with no outputs → Save should stay disabled.
      const save = screen.getByRole("button", { name: /^Save$/ });
      expect(save.hasAttribute("disabled")).toBe(true);

      // Marking the requirement `optional` unblocks the form.
      fireEvent.click(screen.getByLabelText(/requirement optional/i));
      expect(save.hasAttribute("disabled")).toBe(false);
    });

    it("Preview stays clickable while the form is invalid", async () => {
      const previewManifest = vi.fn(async () => mkPreviewResult());
      const { client } = renderEditor("swap", { previewManifest });
      await waitFor(() => expect(client.getAliasTable).toHaveBeenCalled());

      // Default draft is invalid (no id) — Save disabled but Preview live.
      const preview = screen.getByRole("button", { name: /^Preview$/ });
      expect(preview.hasAttribute("disabled")).toBe(false);
      fireEvent.click(preview);
      await waitFor(() => expect(previewManifest).toHaveBeenCalledTimes(1));
    });
  });
});
