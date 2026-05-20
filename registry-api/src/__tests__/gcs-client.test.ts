import { describe, expect, it } from "vitest";
import { classifyGcsError } from "../gcs-client";

describe("classifyGcsError", () => {
  it("maps a 404 code to not_found", () => {
    expect(classifyGcsError({ code: 404 })).toBe("not_found");
  });
  it("maps a string '404' to not_found", () => {
    expect(classifyGcsError({ code: "404" })).toBe("not_found");
  });
  it("maps a 403 (bucket IAM misconfig) to upstream_error", () => {
    expect(classifyGcsError({ code: 403 })).toBe("upstream_error");
  });
  it("maps a network error to upstream_error", () => {
    expect(classifyGcsError(new Error("ECONNRESET"))).toBe("upstream_error");
  });
});
