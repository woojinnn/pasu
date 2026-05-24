/**
 * `declarative-decode` unit tests — selector extraction and route-input assembly.
 *
 * `decodeBundleCalldata` and all related helpers have been removed; calldata
 * decoding now happens inside WASM via `declarative_route_request_json`. This
 * file retains only the `extractSelector` and `buildRouteInput` cases.
 */
import { describe, expect, it } from "vitest";

import { buildRouteInput, extractSelector } from "../declarative-decode";

describe("extractSelector", () => {
  it("returns lowercased 0x + 8 hex for valid calldata", () => {
    expect(extractSelector("0x38ed1739abcd")).toBe("0x38ed1739");
    expect(extractSelector("0x38ED1739abcd")).toBe("0x38ed1739");
  });

  it("returns null for empty or too-short calldata", () => {
    expect(extractSelector(undefined)).toBeNull();
    expect(extractSelector("")).toBeNull();
    expect(extractSelector("0x")).toBeNull();
    expect(extractSelector("0x1234")).toBeNull();
  });

  it("returns null when 0x prefix missing", () => {
    expect(extractSelector("38ed1739")).toBeNull();
  });
});

describe("buildRouteInput", () => {
  const CALLDATA =
    "0x38ed17390000000000000000000000000000000000000000000000000bebc2000000000000000000000000000000000000000000000000000000000000000000";

  it("composes the wire envelope with sensible defaults", () => {
    const input = buildRouteInput({
      chainId: 1,
      to: "0x7a250d5630b4cf539739df2c5dacb4c659f2488d",
      selector: "0x38ed1739",
      from: "0x0000000000000000000000000000000000000001",
      calldata: CALLDATA,
    });

    expect(input.chain_id).toBe(1);
    expect(input.to).toBe("0x7a250d5630b4cf539739df2c5dacb4c659f2488d");
    expect(input.selector).toBe("0x38ed1739");
    expect(input.calldata).toBe(CALLDATA);
    expect(input.ctx).toEqual({
      chain_id: 1,
      from: "0x0000000000000000000000000000000000000001",
      to: "0x7a250d5630b4cf539739df2c5dacb4c659f2488d",
      value_wei: "0",
    });
  });

  it("includes block_timestamp when supplied", () => {
    const input = buildRouteInput({
      chainId: 1,
      to: "0x7a250d5630b4cf539739df2c5dacb4c659f2488d",
      selector: "0x38ed1739",
      from: "0x0000000000000000000000000000000000000001",
      blockTimestamp: 1_700_000_000,
      calldata: CALLDATA,
    });
    expect(input.ctx.block_timestamp).toBe(1_700_000_000);
  });

  it("omits block_timestamp when not supplied", () => {
    const input = buildRouteInput({
      chainId: 1,
      to: "0x7a250d5630b4cf539739df2c5dacb4c659f2488d",
      selector: "0x38ed1739",
      from: "0x0000000000000000000000000000000000000001",
      calldata: CALLDATA,
    });
    expect(input.ctx.block_timestamp).toBeUndefined();
  });
});
