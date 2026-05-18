// Selector-picker tests (Phase 7.1).
//
// The picker walks a hand-coded base schema tree (spec §"Selector roots").
// Two modes:
//   - `params`  → roots: $.root, $.action, $.context
//   - `result`  → root: $.result (free-text suffix, the RPC response shape
//                 is server-defined and not known at design time)
//
// These tests live alongside the component because vitest at the
// browser-extension root picks up `**/*.test.tsx` repo-wide. The
// dashboard package has no separate test runner.

import { describe, expect, it, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { SelectorPicker } from "./selector-picker";

describe("SelectorPicker (params mode)", () => {
  it("renders the three params roots", () => {
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
    const onChange = vi.fn();
    render(
      <SelectorPicker
        mode="params"
        action="swap"
        value=""
        onChange={onChange}
      />,
    );
    // Expand $.root.
    fireEvent.click(screen.getByText("$.root"));
    // Pick chain_id from the expanded list.
    fireEvent.click(screen.getByText("chain_id"));
    expect(onChange).toHaveBeenCalledWith("$.root.chain_id");
  });

  it("expands $.action for swap and emits inputToken", () => {
    const onChange = vi.fn();
    render(
      <SelectorPicker
        mode="params"
        action="swap"
        value=""
        onChange={onChange}
      />,
    );
    fireEvent.click(screen.getByText("$.action"));
    fireEvent.click(screen.getByText("inputToken"));
    expect(onChange).toHaveBeenCalledWith("$.action.inputToken");
  });

  it("falls back to a generic $.action.* leaf for unknown actions", () => {
    // Unknown action — picker doesn't know the action envelope shape, so
    // it surfaces a free-text leaf the user can edit. We don't crash.
    const onChange = vi.fn();
    render(
      <SelectorPicker
        mode="params"
        action="totally_unknown_action"
        value=""
        onChange={onChange}
      />,
    );
    fireEvent.click(screen.getByText("$.action"));
    const input = screen.getByLabelText(/custom \$\.action path/i);
    fireEvent.change(input, { target: { value: "$.action.foo" } });
    expect(onChange).toHaveBeenCalledWith("$.action.foo");
  });

  it("does NOT render $.result in params mode", () => {
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

describe("SelectorPicker (result mode)", () => {
  it("renders only $.result root (RPC response shape is server-defined)", () => {
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
    const onChange = vi.fn();
    render(
      <SelectorPicker
        mode="result"
        action="swap"
        value=""
        onChange={onChange}
      />,
    );
    // Expand $.result first — the suffix input is revealed under it.
    fireEvent.click(screen.getByText("$.result"));
    const input = screen.getByLabelText(/\$\.result path/i);
    fireEvent.change(input, { target: { value: "$.result.usd_value" } });
    expect(onChange).toHaveBeenCalledWith("$.result.usd_value");
  });
});
