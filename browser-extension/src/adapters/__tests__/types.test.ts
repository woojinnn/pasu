import { describe, expect, it } from "vitest";
import type { Manifest, DecodedCall } from "../types";

describe("type shapes", () => {
  it("Manifest accepts the canonical example", () => {
    const m: Manifest = {
      name: "erc20-transfer",
      version: "0.1.0",
      sdk_version: 1,
      description: "x",
      capabilities: ["decoder", "call_adapter"],
      applies_to: [{ chain: 1, address: "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48" }],
      factory_of: [],
      proxy_of: [],
    };
    expect(m.name).toBe("erc20-transfer");
  });

  it("DecodedCall round-trips through JSON", () => {
    const d: DecodedCall = {
      chain_id: 1,
      target: "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
      selector: "0xa9059cbb",
      function: "transfer",
      args: [{ name: "to", value: { type: "address", value: "0x0000000000000000000000000000000000000001" } }],
      nested: [],
    };
    const back = JSON.parse(JSON.stringify(d)) as DecodedCall;
    expect(back.function).toBe("transfer");
  });
});
