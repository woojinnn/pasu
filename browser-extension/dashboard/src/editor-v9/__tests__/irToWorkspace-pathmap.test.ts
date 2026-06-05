/**
 * Integration test: irToWorkspace records the Expr→blockId identity map
 * correctly, and the pathToBlockId combiner surfaces live Blockly block ids.
 *
 * jsdom does not implement HTMLCanvasElement.getContext (canvas2d), which
 * Blockly uses only for text-width measurement during render(). Block ids are
 * assigned at newBlock() time, so the identity map is unaffected by rendering.
 * The stub below makes render() complete without throwing.
 */

// @vitest-environment jsdom
import { describe, it, expect, beforeAll } from "vitest";
import * as Blockly from "blockly";
import { registerBlocks } from "../blocks/register";
import { irToWorkspace } from "../mapping/irToWorkspace";
import { pathToBlockId } from "../../cedar/diagnosis/path";
import type { PolicyIR, Expr } from "../../cedar/blocks/ir";

// Stub canvas 2d context — jsdom returns null for getContext("2d").
// Blockly's text-width path sets ctx.font and calls ctx.measureText(), both
// of which are no-ops here. Rendering completes; block ids stay valid.
Object.defineProperty(HTMLCanvasElement.prototype, "getContext", {
  value: () => ({
    font: "",
    measureText: () => ({ width: 0 }),
    fillText: () => {},
    clearRect: () => {},
  }),
  writable: true,
});

describe("irToWorkspace → pathToBlockId identity-map integration", () => {
  let ws: Blockly.WorkspaceSvg;

  beforeAll(() => {
    registerBlocks();
    const div = document.createElement("div");
    document.body.appendChild(div);
    ws = Blockly.inject(div, { readOnly: false });
  });

  it("pathToBlockId('c0.body') resolves to a live block for forbid when { context.slippageBp > 100 }", () => {
    // IR for: forbid when { context.slippageBp > 100 }
    const body: Expr = {
      kind: "binary",
      op: ">",
      left: { kind: "attr", of: { kind: "var", name: "context" }, attr: "slippageBp" },
      right: { kind: "lit", litType: "long", value: 100 },
    };
    const policy: PolicyIR = {
      kind: "policy",
      effect: "forbid",
      annotations: [],
      scope: {
        principal: { kind: "scopeAll" },
        action: { kind: "scopeAll" },
        resource: { kind: "scopeAll" },
      },
      conditions: [{ kind: "when", body }],
    };

    const blockIdByNode = new Map<Expr, string>();
    irToWorkspace(ws, [policy], blockIdByNode);

    const pathMap = pathToBlockId(policy, blockIdByNode);

    // c0.body is the binary comparison block — must be recorded and live.
    const bodyId = pathMap.get("c0.body");
    expect(bodyId).toBeTruthy();
    expect(ws.getBlockById(bodyId!)).not.toBeNull();
  });
});
