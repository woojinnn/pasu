import { describe, expect, it } from "vitest";
import {
  locallyHandledMethods,
  tryHandleLocally,
  type LocalRpcResult,
} from "../local-method-handlers";

function call(method: string, params: unknown, id = "call-0") {
  return { id, method, params };
}

function unwrap(result: LocalRpcResult | null): {
  ok: boolean;
  body: unknown;
} {
  if (!result) throw new Error("expected a local result, got null");
  if (result.ok) return { ok: true, body: result.result };
  return { ok: false, body: result.error };
}

describe("local-method-handlers", () => {
  describe("tryHandleLocally", () => {
    it("returns null for methods the SW doesn't handle locally", () => {
      const out = tryHandleLocally(call("oracle.usd_value", {}));
      expect(out).toBeNull();
    });

    it("advertises its handler set so callers can audit which calls bypass HTTP", () => {
      const methods = locallyHandledMethods();
      expect(methods).toContain("token.normalize_to_nano");
    });
  });

  describe("token.normalize_to_nano", () => {
    it("multiplies low-decimal amounts up to the 10⁻⁹ scale (USDC, decimals=6)", () => {
      // 100 USDC: 100 × 10⁶ raw → 100 × 10⁹ at scale=9
      const out = unwrap(
        tryHandleLocally(
          call("token.normalize_to_nano", { amount: "100000000", decimals: 6 }),
        ),
      );
      expect(out).toEqual({ ok: true, body: { nano: 100_000_000_000 } });
    });

    it("divides high-decimal amounts down to the 10⁻⁹ scale (ETH, decimals=18)", () => {
      // 0.00003 ETH: 30,000,000,000,000 wei → 30,000 at scale=9
      const out = unwrap(
        tryHandleLocally(
          call("token.normalize_to_nano", {
            amount: "30000000000000",
            decimals: 18,
          }),
        ),
      );
      expect(out).toEqual({ ok: true, body: { nano: 30_000 } });
    });

    it("is identity when decimals == scale (decimals=9)", () => {
      const out = unwrap(
        tryHandleLocally(
          call("token.normalize_to_nano", { amount: "1234567", decimals: 9 }),
        ),
      );
      expect(out).toEqual({ ok: true, body: { nano: 1_234_567 } });
    });

    it("returns the same nano value for the same human-facing amount across decimals", () => {
      // 1 token in different decimal systems must hit 10⁹ at scale=9.
      const cases: { amount: string; decimals: number }[] = [
        { amount: "1000000000000000000", decimals: 18 }, // ETH
        { amount: "1000000", decimals: 6 }, // USDC
        { amount: "100000000", decimals: 8 }, // WBTC
      ];
      for (const params of cases) {
        const out = unwrap(tryHandleLocally(call("token.normalize_to_nano", params)));
        expect(out).toEqual({ ok: true, body: { nano: 1_000_000_000 } });
      }
    });

    it("rejects non-string amount", () => {
      const out = unwrap(
        tryHandleLocally(
          call("token.normalize_to_nano", { amount: 100, decimals: 6 }),
        ),
      );
      expect(out.ok).toBe(false);
      expect((out.body as { code: string }).code).toBe("invalid_params");
    });

    it("rejects negative amount", () => {
      const out = unwrap(
        tryHandleLocally(
          call("token.normalize_to_nano", { amount: "-100", decimals: 6 }),
        ),
      );
      expect(out.ok).toBe(false);
    });

    it("rejects out-of-range decimals", () => {
      const overshoot = unwrap(
        tryHandleLocally(
          call("token.normalize_to_nano", { amount: "1", decimals: 31 }),
        ),
      );
      expect(overshoot.ok).toBe(false);
      const negative = unwrap(
        tryHandleLocally(
          call("token.normalize_to_nano", { amount: "1", decimals: -1 }),
        ),
      );
      expect(negative.ok).toBe(false);
    });

    it("flags overflow when rescaled value exceeds Number.MAX_SAFE_INTEGER", () => {
      // 10 quadrillion USDC at decimals=6 → 10¹⁶ × 10³ = 10¹⁹, way past 2⁵³.
      const out = unwrap(
        tryHandleLocally(
          call("token.normalize_to_nano", {
            amount: "10000000000000000000000",
            decimals: 6,
          }),
        ),
      );
      expect(out.ok).toBe(false);
      expect((out.body as { code: string }).code).toBe("overflow");
    });

    it("rejects malformed amount string", () => {
      const out = unwrap(
        tryHandleLocally(
          call("token.normalize_to_nano", { amount: "1.5", decimals: 6 }),
        ),
      );
      expect(out.ok).toBe(false);
    });

    it("preserves the call id so the WASM materializer can correlate", () => {
      const result = tryHandleLocally(
        call("token.normalize_to_nano", { amount: "1", decimals: 9 }, "abc-123"),
      );
      expect(result?.id).toBe("abc-123");
    });

    it("DEFERS to the remote server (returns null) when decimals is omitted", () => {
      // The registry-driven path passes `{ amount, chain_id, asset }` with NO
      // literal decimals — the host then routes to /evaluate, which resolves the
      // token's real decimals globally. Returning null (not an error) is what
      // makes the call fall through to the remote dispatcher.
      const result = tryHandleLocally(
        call("token.normalize_to_nano", {
          amount: "60000",
          chain_id: "eip155:1",
          asset: "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
        }),
      );
      expect(result).toBeNull();
    });
  });
});
