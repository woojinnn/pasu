// Selector-picker tests (Phase 7.1, extended in Phase 8.5).
//
// Modes:
//   - `params`  → roots: $.root, $.action, $.context. $.action and
//                 $.context expose the action's schema tree pulled
//                 from policy-builder via WASM, so users can drill
//                 into nested records (`inputToken.asset`,
//                 `inputToken.amount.value`, etc.) — addresses the
//                 Phase 8.5 finding that the picker stopped at
//                 top-level fields.
//   - `result`  → root: $.result with a free-text suffix (RPC
//                 response shape is server-defined).
//
// We mock `fetchActionSchema` so tests don't load the real WASM.
// The mock returns a small swap-shaped fixture covering the nested
// shape the production schema declares.

import { describe, expect, it, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import type { ActionSchemaDto, FieldDto } from "../policy/types";

// `vi.hoisted` runs before any `import` (vitest moves it to the top of
// the module graph) so the mock factory below can close over the spy
// without hitting a TDZ on the boundary `vi.mock` rewrites in.
const mocks = vi.hoisted(() => ({
  fetchActionSchema: vi.fn(),
  fetchTypedPaths: vi.fn(),
}));

vi.mock("../policy/builder-wasm", () => ({
  fetchActionSchema: mocks.fetchActionSchema,
  fetchTypedPaths: mocks.fetchTypedPaths,
}));

import { SelectorPicker } from "./selector-picker";

// Minimal swap-shaped schema — base fields only, with the nested
// records the new drill-down test exercises. We intentionally include
// one custom field (`totalInputUsd.*`) to assert the picker filters
// it out (custom belongs in `context.custom.*`, not addressable from
// `$.action.*`).
function fakeSwapSchema(): ActionSchemaDto {
  return {
    action: "swap",
    principalType: "Wallet",
    resourceType: "Protocol",
    fields: [
      fld("swapMode", "string"),
      fld("recipient", "string"),
      fld("feeBps", "long"),
      fld("inputToken.asset.kind", "string"),
      fld("inputToken.asset.address", "string"),
      fld("inputToken.asset.symbol", "string"),
      fld("inputToken.asset.decimals", "long"),
      fld("inputToken.amount.kind", "string"),
      fld("inputToken.amount.value", "string"),
      fld("outputToken.asset.kind", "string"),
      fld("outputToken.amount.value", "string"),
      fld("validity.expiresAt", "string"),
      fld("validity.source", "string"),
      // Custom — picker must filter out.
      { ...fld("totalInputUsd.value", "decimal"), isCustom: true },
    ],
  };
}

function fld(path: string, type: FieldDto["type"]): FieldDto {
  return {
    path,
    type,
    optional: false,
    parentOptional: false,
    isCustom: false,
    operators: [],
  };
}

function setSchema(schema: ActionSchemaDto | undefined) {
  mocks.fetchActionSchema.mockResolvedValue(
    schema ? { schema } : { error: { kind: "unknown_action" } },
  );
}

function fakeTypedPaths() {
  return {
    paths: {
      action: "swap",
      scalars: [
        { path: "$.root.chain_id", cedarType: "long" },
        { path: "$.root.from", cedarType: "string" },
        { path: "$.root.value_wei", cedarType: "string" },
        { path: "$.root.block_timestamp", cedarType: "long" },
        { path: "$.action.feeBps", cedarType: "long" },
        { path: "$.action.inputToken.asset.decimals", cedarType: "long" },
        { path: "$.action.inputToken.asset.address", cedarType: "string" },
        { path: "$.action.inputToken.asset.symbol", cedarType: "string" },
        { path: "$.action.inputToken.amount.value", cedarType: "string" },
        { path: "$.action.recipient", cedarType: "string" },
      ],
      records: [
        { path: "$.action.inputToken", cedarAlias: "AssetRefWithAmountConstraint" },
        { path: "$.action.inputToken.asset", cedarAlias: "AssetRef" },
        { path: "$.action.inputToken.amount", cedarAlias: "AmountConstraint" },
        { path: "$.action.outputToken.asset", cedarAlias: "AssetRef" },
        { path: "$.action.validity", cedarAlias: "Validity" },
      ],
    },
  };
}

function setTypedPaths(payload: ReturnType<typeof fakeTypedPaths> | { error: object }) {
  mocks.fetchTypedPaths.mockResolvedValue(payload);
}

describe("SelectorPicker (params mode)", () => {
  it("renders the three params roots", () => {
    setSchema(fakeSwapSchema());
    render(
      <SelectorPicker
        mode="params"
        action="swap"
        value=""
        onChange={() => {}}
      />,
    );
    expect(screen.getByText("$.root")).toBeTruthy();
    expect(screen.getByText("$.action")).toBeTruthy();
    expect(screen.getByText("$.context")).toBeTruthy();
  });

  it("expands $.root and emits a chain_id selection", () => {
    setSchema(fakeSwapSchema());
    const onChange = vi.fn();
    render(
      <SelectorPicker
        mode="params"
        action="swap"
        value=""
        onChange={onChange}
      />,
    );
    fireEvent.click(screen.getByText("$.root"));
    fireEvent.click(screen.getByText("chain_id"));
    expect(onChange).toHaveBeenCalledWith("$.root.chain_id");
  });

  it("expands $.action and lets the user pick a top-level composite (e.g. inputToken)", async () => {
    setSchema(fakeSwapSchema());
    const onChange = vi.fn();
    render(
      <SelectorPicker
        mode="params"
        action="swap"
        value=""
        onChange={onChange}
      />,
    );
    // Wait for the async fetch to land — the tree only renders after.
    await waitFor(() =>
      expect(mocks.fetchActionSchema).toHaveBeenCalledWith("swap"),
    );
    fireEvent.click(screen.getByText("$.action"));
    // Wait for inputToken to appear (schema-driven).
    const inputToken = await screen.findByRole("button", { name: "inputToken" });
    fireEvent.click(inputToken);
    expect(onChange).toHaveBeenCalledWith("$.action.inputToken");
  });

  it("drills into nested composites so leaves like inputToken.asset.address are reachable", async () => {
    // The Phase 8.5 regression bait: previous picker stopped at
    // top-level fields, so users couldn't reach String leaves under
    // `inputToken.asset.*` from the picker. This test pins the fix.
    setSchema(fakeSwapSchema());
    const onChange = vi.fn();
    render(
      <SelectorPicker
        mode="params"
        action="swap"
        value=""
        onChange={onChange}
      />,
    );
    await waitFor(() =>
      expect(mocks.fetchActionSchema).toHaveBeenCalledWith("swap"),
    );
    fireEvent.click(screen.getByText("$.action"));
    // inputToken is a composite — drilling reveals `asset` and `amount`.
    const expandInputToken = await screen.findAllByRole("button", {
      name: /expand/i,
    });
    // inputToken's chevron is the first composite that lists asset+amount.
    // Click the chevron next to "inputToken".
    const inputTokenLabelBtn = screen.getByRole("button", {
      name: "inputToken",
    });
    const inputTokenChevron = inputTokenLabelBtn
      .parentElement!.querySelector("button[aria-label='expand']")!;
    fireEvent.click(inputTokenChevron);
    // Now `asset` and `amount` appear; expand `asset`.
    const assetLabelBtn = await screen.findByRole("button", { name: "asset" });
    const assetChevron = assetLabelBtn
      .parentElement!.querySelector("button[aria-label='expand']")!;
    fireEvent.click(assetChevron);
    // `address` leaf is reachable.
    const addressLeaf = await screen.findByRole("button", { name: "address" });
    fireEvent.click(addressLeaf);
    expect(onChange).toHaveBeenCalledWith(
      "$.action.inputToken.asset.address",
    );
    expect(expandInputToken.length).toBeGreaterThan(0);
  });

  it("filters out custom fields from the $.action subtree (those belong to $.context.custom)", async () => {
    setSchema(fakeSwapSchema());
    render(
      <SelectorPicker
        mode="params"
        action="swap"
        value=""
        onChange={() => {}}
      />,
    );
    await waitFor(() => expect(mocks.fetchActionSchema).toHaveBeenCalled());
    fireEvent.click(screen.getByText("$.action"));
    // `totalInputUsd` is a custom field in our fixture — must NOT appear.
    expect(screen.queryByRole("button", { name: "totalInputUsd" })).toBeNull();
  });

  it("falls back to a custom-path input when the schema is unavailable", async () => {
    setSchema(undefined); // Schema lookup fails.
    const onChange = vi.fn();
    render(
      <SelectorPicker
        mode="params"
        action="totally_unknown_action"
        value=""
        onChange={onChange}
      />,
    );
    await waitFor(() => expect(mocks.fetchActionSchema).toHaveBeenCalled());
    fireEvent.click(screen.getByText("$.action"));
    // Empty tree → custom path input renders.
    const input = await screen.findByLabelText(/custom selector suffix/i);
    fireEvent.change(input, {
      target: { value: "inputToken.asset.address" },
    });
    expect(onChange).toHaveBeenCalledWith(
      "$.action.inputToken.asset.address",
    );
  });

  it("does NOT render $.result in params mode", () => {
    setSchema(fakeSwapSchema());
    render(
      <SelectorPicker
        mode="params"
        action="swap"
        value=""
        onChange={() => {}}
      />,
    );
    expect(screen.queryByText("$.result")).toBeNull();
  });

  it("displays the current value", () => {
    setSchema(fakeSwapSchema());
    render(
      <SelectorPicker
        mode="params"
        action="swap"
        value="$.root.chain_id"
        onChange={() => {}}
      />,
    );
    expect(screen.getByDisplayValue("$.root.chain_id")).toBeTruthy();
  });
});

describe("SelectorPicker (typed slot — Phase 8.5 PR 4)", () => {
  it("renders only Long-typed paths when requiredType='Long' (list open because value is empty)", async () => {
    setSchema(fakeSwapSchema());
    setTypedPaths(fakeTypedPaths());
    render(
      <SelectorPicker
        mode="params"
        action="swap"
        value=""
        requiredType="Long"
        onChange={() => {}}
      />,
    );
    await waitFor(() => expect(mocks.fetchTypedPaths).toHaveBeenCalledWith("swap"));
    // All four Long paths from the fixture appear.
    expect(await screen.findByRole("button", { name: "$.root.chain_id" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "$.root.block_timestamp" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "$.action.feeBps" })).toBeTruthy();
    expect(
      screen.getByRole("button", { name: "$.action.inputToken.asset.decimals" }),
    ).toBeTruthy();
    // String paths are filtered out — this used to be the bug bait.
    expect(screen.queryByRole("button", { name: "$.root.from" })).toBeNull();
    expect(
      screen.queryByRole("button", { name: "$.action.inputToken.amount.value" }),
    ).toBeNull();
    expect(screen.queryByRole("button", { name: "$.action.recipient" })).toBeNull();
  });

  it("collapses the option list when the current value is already a valid pick", async () => {
    setSchema(fakeSwapSchema());
    setTypedPaths(fakeTypedPaths());
    render(
      <SelectorPicker
        mode="params"
        action="swap"
        // Pre-set value matches one of the Long-typed paths — list
        // should start collapsed so the 4-param form isn't 4 tall
        // expanded blocks tall.
        value="$.root.chain_id"
        requiredType="Long"
        onChange={() => {}}
      />,
    );
    await waitFor(() => expect(mocks.fetchTypedPaths).toHaveBeenCalled());
    // The currently-selected path is the value of the text input but
    // the option buttons are hidden until the user clicks the chevron.
    expect(screen.queryByRole("button", { name: "$.root.block_timestamp" })).toBeNull();
    expect(screen.getByDisplayValue("$.root.chain_id")).toBeTruthy();
    // Chevron toggle is rendered and labelled.
    const toggle = screen.getByLabelText(/show options/i);
    fireEvent.click(toggle);
    // After expanding, the options reappear.
    expect(screen.getByRole("button", { name: "$.root.block_timestamp" })).toBeTruthy();
  });

  it("auto-collapses when an invalid value transitions to a valid pick (e.g. method swap)", async () => {
    // Reproduces the carry-over: method swap injects a valid default
    // selector but the list was open from the previous method's
    // invalid-during-typing state. New code auto-closes on every
    // value/matchingPaths transition so the freshly-populated form
    // doesn't render N expanded blocks.
    setSchema(fakeSwapSchema());
    setTypedPaths(fakeTypedPaths());
    const { rerender } = render(
      <SelectorPicker
        mode="params"
        action="swap"
        value=""
        requiredType="Long"
        onChange={() => {}}
      />,
    );
    await waitFor(() => expect(mocks.fetchTypedPaths).toHaveBeenCalled());
    // Empty value → list open.
    expect(await screen.findByRole("button", { name: "$.root.chain_id" })).toBeTruthy();
    // Method swap simulated: parent injects a valid default.
    rerender(
      <SelectorPicker
        mode="params"
        action="swap"
        value="$.root.chain_id"
        requiredType="Long"
        onChange={() => {}}
      />,
    );
    // Auto-closes — the other options vanish.
    expect(screen.queryByRole("button", { name: "$.root.block_timestamp" })).toBeNull();
  });

  it("re-opens automatically when the value becomes invalid (e.g. user clears input)", async () => {
    setSchema(fakeSwapSchema());
    setTypedPaths(fakeTypedPaths());
    const { rerender } = render(
      <SelectorPicker
        mode="params"
        action="swap"
        value="$.root.chain_id"
        requiredType="Long"
        onChange={() => {}}
      />,
    );
    await waitFor(() => expect(mocks.fetchTypedPaths).toHaveBeenCalled());
    // Initially collapsed.
    expect(screen.queryByRole("button", { name: "$.root.block_timestamp" })).toBeNull();
    // User clears the input.
    rerender(
      <SelectorPicker
        mode="params"
        action="swap"
        value=""
        requiredType="Long"
        onChange={() => {}}
      />,
    );
    // List auto-opens because the empty string isn't in matchingPaths.
    expect(screen.getByRole("button", { name: "$.root.block_timestamp" })).toBeTruthy();
  });

  it("renders only AssetRef-typed composite paths when requiredType='AssetRef'", async () => {
    setSchema(fakeSwapSchema());
    setTypedPaths(fakeTypedPaths());
    render(
      <SelectorPicker
        mode="params"
        action="swap"
        value=""
        requiredType="AssetRef"
        onChange={() => {}}
      />,
    );
    await waitFor(() => expect(mocks.fetchTypedPaths).toHaveBeenCalledWith("swap"));
    expect(await screen.findByRole("button", { name: "$.action.inputToken.asset" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "$.action.outputToken.asset" })).toBeTruthy();
    // Other record aliases (Validity, AssetRefWithAmountConstraint) and
    // every scalar are absent.
    expect(screen.queryByRole("button", { name: "$.action.validity" })).toBeNull();
    expect(screen.queryByRole("button", { name: "$.action.inputToken" })).toBeNull();
    expect(screen.queryByRole("button", { name: "$.root.chain_id" })).toBeNull();
  });

  it("clicking a typed path emits its full $.* selector", async () => {
    setSchema(fakeSwapSchema());
    setTypedPaths(fakeTypedPaths());
    const onChange = vi.fn();
    render(
      <SelectorPicker
        mode="params"
        action="swap"
        value=""
        requiredType="Long"
        onChange={onChange}
      />,
    );
    await waitFor(() => expect(mocks.fetchTypedPaths).toHaveBeenCalled());
    fireEvent.click(await screen.findByRole("button", { name: "$.root.chain_id" }));
    expect(onChange).toHaveBeenCalledWith("$.root.chain_id");
  });

  it("the top text input is still editable as an escape hatch", async () => {
    setSchema(fakeSwapSchema());
    setTypedPaths(fakeTypedPaths());
    const onChange = vi.fn();
    render(
      <SelectorPicker
        mode="params"
        action="swap"
        value=""
        requiredType="Long"
        onChange={onChange}
      />,
    );
    await waitFor(() => expect(mocks.fetchTypedPaths).toHaveBeenCalled());
    const input = screen.getByLabelText(/Selector path/i);
    fireEvent.change(input, { target: { value: "$.root.custom_thing" } });
    expect(onChange).toHaveBeenCalledWith("$.root.custom_thing");
  });

  it("falls back to the untyped tree when typed-paths fetch hasn't landed yet", async () => {
    setSchema(fakeSwapSchema());
    // Typed paths mock returns a never-resolving promise — picker must
    // not block on it.
    mocks.fetchTypedPaths.mockReturnValue(new Promise(() => {}));
    render(
      <SelectorPicker
        mode="params"
        action="swap"
        value=""
        requiredType="Long"
        onChange={() => {}}
      />,
    );
    // Untyped tree's `$.root` button still renders.
    expect(screen.getByText("$.root")).toBeTruthy();
    expect(screen.getByText("$.action")).toBeTruthy();
  });

  it("typing in the input filters the typed-mode list to substring matches", async () => {
    setSchema(fakeSwapSchema());
    setTypedPaths(fakeTypedPaths());
    render(
      <SelectorPicker
        mode="params"
        action="swap"
        // Pre-set value mimics the user typing `$.ro` to narrow the list.
        value="$.ro"
        requiredType="String"
        onChange={() => {}}
      />,
    );
    await waitFor(() => expect(mocks.fetchTypedPaths).toHaveBeenCalled());
    // `$.root.from` and `$.root.value_wei` (String paths under $.root) appear.
    expect(await screen.findByRole("button", { name: "$.root.from" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "$.root.value_wei" })).toBeTruthy();
    // `$.action.recipient` (also String, but doesn't contain `$.ro`) is hidden.
    expect(screen.queryByRole("button", { name: "$.action.recipient" })).toBeNull();
    expect(
      screen.queryByRole("button", { name: "$.action.inputToken.asset.address" }),
    ).toBeNull();
  });

  it("falls back to the full list when no path matches the typed filter", async () => {
    setSchema(fakeSwapSchema());
    setTypedPaths(fakeTypedPaths());
    render(
      <SelectorPicker
        mode="params"
        action="swap"
        value="zzz-no-matches"
        requiredType="String"
        onChange={() => {}}
      />,
    );
    await waitFor(() => expect(mocks.fetchTypedPaths).toHaveBeenCalled());
    // No path contains "zzz-no-matches" → reverts to showing the full
    // String list so the user isn't stranded.
    expect(await screen.findByRole("button", { name: "$.root.from" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "$.action.recipient" })).toBeTruthy();
    expect(screen.getByText(/No path contains/i)).toBeTruthy();
  });

  it("clears the filter when the input is emptied", async () => {
    setSchema(fakeSwapSchema());
    setTypedPaths(fakeTypedPaths());
    const onChange = vi.fn();
    const { rerender } = render(
      <SelectorPicker
        mode="params"
        action="swap"
        value="$.root"
        requiredType="String"
        onChange={onChange}
      />,
    );
    await waitFor(() => expect(mocks.fetchTypedPaths).toHaveBeenCalled());
    // Only `$.root.*` String paths visible.
    expect(await screen.findByRole("button", { name: "$.root.from" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "$.action.recipient" })).toBeNull();

    // Simulate the user clearing the input.
    rerender(
      <SelectorPicker
        mode="params"
        action="swap"
        value=""
        requiredType="String"
        onChange={onChange}
      />,
    );
    // Full list returns.
    expect(screen.getByRole("button", { name: "$.action.recipient" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "$.root.from" })).toBeTruthy();
  });

  it("shows an explanatory empty state (not the untyped tree) when type has zero matches", async () => {
    setSchema(fakeSwapSchema());
    // Empty scalars/records — typed mode stays active.
    mocks.fetchTypedPaths.mockResolvedValue({
      paths: { action: "swap", scalars: [], records: [] },
    });
    render(
      <SelectorPicker
        mode="params"
        action="swap"
        value=""
        requiredType="UnseenType"
        onChange={() => {}}
      />,
    );
    await waitFor(() => expect(mocks.fetchTypedPaths).toHaveBeenCalled());
    // The untyped tree's $.root/$.action buttons must NOT appear —
    // falling back to that mode confused users (PR4 carry-over).
    expect(screen.queryByText("$.root")).toBeNull();
    expect(screen.queryByText("$.action")).toBeNull();
    // Empty state explanation surfaced instead.
    expect(
      screen.getByText(/The action exposes no path of type/i),
    ).toBeTruthy();
  });

  it("Bool-typed params render `true` / `false` buttons when the action has no Bool path", async () => {
    setSchema(fakeSwapSchema());
    mocks.fetchTypedPaths.mockResolvedValue({
      paths: { action: "swap", scalars: [], records: [] },
    });
    const onChange = vi.fn();
    render(
      <SelectorPicker
        mode="params"
        action="swap"
        value=""
        requiredType="Bool"
        onChange={onChange}
      />,
    );
    await waitFor(() => expect(mocks.fetchTypedPaths).toHaveBeenCalled());
    // Two explicit literal buttons. The manifest serializer coerces
    // the picked string to a JSON boolean downstream — UI just emits
    // the literal `"true"` / `"false"` here.
    const trueBtn = screen.getByRole("button", { name: "true" });
    const falseBtn = screen.getByRole("button", { name: "false" });
    fireEvent.click(trueBtn);
    expect(onChange).toHaveBeenCalledWith("true");
    fireEvent.click(falseBtn);
    expect(onChange).toHaveBeenCalledWith("false");
  });

  it("Bool picker highlights the currently-selected literal", async () => {
    setSchema(fakeSwapSchema());
    mocks.fetchTypedPaths.mockResolvedValue({
      paths: { action: "swap", scalars: [], records: [] },
    });
    render(
      <SelectorPicker
        mode="params"
        action="swap"
        value="true"
        requiredType="Bool"
        onChange={() => {}}
      />,
    );
    await waitFor(() => expect(mocks.fetchTypedPaths).toHaveBeenCalled());
    const trueBtn = screen.getByRole("button", { name: "true" });
    expect(trueBtn.className).toContain("selector-bool-option-active");
    const falseBtn = screen.getByRole("button", { name: "false" });
    expect(falseBtn.className).not.toContain("selector-bool-option-active");
  });
});

describe("SelectorPicker (result mode)", () => {
  it("renders only $.result root (RPC response shape is server-defined)", () => {
    setSchema(fakeSwapSchema());
    render(
      <SelectorPicker
        mode="result"
        action="swap"
        value=""
        onChange={() => {}}
      />,
    );
    expect(screen.getByText("$.result")).toBeTruthy();
    expect(screen.queryByText("$.root")).toBeNull();
    expect(screen.queryByText("$.action")).toBeNull();
    expect(screen.queryByText("$.context")).toBeNull();
  });

  it("emits $.result-rooted selectors via the suffix input", () => {
    setSchema(fakeSwapSchema());
    const onChange = vi.fn();
    render(
      <SelectorPicker
        mode="result"
        action="swap"
        value=""
        onChange={onChange}
      />,
    );
    fireEvent.click(screen.getByText("$.result"));
    const input = screen.getByLabelText(/\$\.result path/i);
    fireEvent.change(input, { target: { value: "$.result.usd_value" } });
    expect(onChange).toHaveBeenCalledWith("$.result.usd_value");
  });

  it("doesn't fetch the action schema (result mode doesn't need it)", () => {
    setSchema(fakeSwapSchema());
    mocks.fetchActionSchema.mockClear();
    render(
      <SelectorPicker
        mode="result"
        action="swap"
        value=""
        onChange={() => {}}
      />,
    );
    expect(mocks.fetchActionSchema).not.toHaveBeenCalled();
  });
});
