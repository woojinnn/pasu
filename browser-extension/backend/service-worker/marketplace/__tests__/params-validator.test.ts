import { describe, expect, it } from "vitest";
import {
  defaultsFor,
  validateParams,
  type ParamsSchema,
} from "../params-validator";

describe("validateParams", () => {
  it("accepts a well-formed integer + address pair", () => {
    const schema: ParamsSchema = {
      cap: { type: "integer", min: 0, max: 100 },
      spender: { type: "address" },
    };
    expect(() =>
      validateParams(schema, {
        cap: 42,
        spender: "0x1111111111111111111111111111111111111111",
      }),
    ).not.toThrow();
  });

  it("rejects out-of-range integers", () => {
    const schema: ParamsSchema = { x: { type: "integer", min: 0, max: 10 } };
    expect(() => validateParams(schema, { x: 999 })).toThrow(/outside/);
  });

  it("rejects undeclared params", () => {
    const schema: ParamsSchema = { x: { type: "integer", min: 0, max: 1 } };
    expect(() => validateParams(schema, { x: 1, y: 2 })).toThrow(
      /not declared/,
    );
  });

  it("rejects malformed addresses", () => {
    const schema: ParamsSchema = { addr: { type: "address" } };
    expect(() => validateParams(schema, { addr: "0xtoo-short" })).toThrow(
      /address/,
    );
  });
});

describe("defaultsFor", () => {
  it("uses schema defaults", () => {
    const schema: ParamsSchema = {
      cap: { type: "integer", min: 0, max: 100, default: 25 },
    };
    expect(defaultsFor(schema)).toEqual({ cap: 25 });
  });

  it("applies overrides on top of defaults", () => {
    const schema: ParamsSchema = {
      cap: { type: "integer", min: 0, max: 100, default: 25 },
      label: {
        type: "string",
        maxLen: 16,
        allowedChars: "A-Za-z",
        default: "demo",
      },
    };
    expect(defaultsFor(schema, { cap: 50 })).toEqual({
      cap: 50,
      label: "demo",
    });
  });

  it("throws when a param has no default and no override", () => {
    const schema: ParamsSchema = { cap: { type: "integer", min: 0, max: 100 } };
    expect(() => defaultsFor(schema)).toThrow(/no default/);
  });
});
