/**
 * `declarative-decode` unit tests — selector extraction.
 *
 * Calldata decoding moved into WASM (`declarative_route_request_v3_json`).
 * Post-B4 cleanup (commits 6aa3cc0 / b6f3ac9): v1 `buildRouteInput` +
 * `DeclarativeRouteRequestInput` removed.
 */
import { describe, expect, it } from "vitest";

import { extractSelector } from "../declarative-decode";

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
