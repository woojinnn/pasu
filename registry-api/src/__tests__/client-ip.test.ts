import { describe, expect, it } from "vitest";
import { extractClientIp } from "../server";

describe("extractClientIp (X-Forwarded-For trusted-hop selection — threat model A1)", () => {
  it("takes the rightmost entry by default (trustedProxyHops=0)", () => {
    // rightmost = the hop the trusted edge appended (the genuine client IP on
    // direct *.run.app); on Cloud Run it is never the client-supplied left side.
    expect(extractClientIp("9.9.9.9, 8.8.8.8", "127.0.0.1", 0)).toBe("8.8.8.8");
  });

  it("ignores a rotating spoofable leftmost value", () => {
    // Attacker controls the left; the trusted rightmost is what we key on.
    const a = extractClientIp("1.1.1.1, 8.8.8.8", "127.0.0.1", 0);
    const b = extractClientIp("2.2.2.2, 8.8.8.8", "127.0.0.1", 0);
    expect(a).toBe("8.8.8.8");
    expect(b).toBe("8.8.8.8");
    expect(a).toBe(b);
  });

  it("counts trustedProxyHops in from the right (LB topology)", () => {
    // [client-supplied, realClient, lbHop] with one extra trusted hop.
    expect(extractClientIp("1.1.1.1, 5.5.5.5, 6.6.6.6", "127.0.0.1", 1)).toBe(
      "5.5.5.5",
    );
  });

  it("falls back to the socket address when XFF is absent", () => {
    expect(extractClientIp(undefined, "203.0.113.7", 0)).toBe("203.0.113.7");
    expect(extractClientIp("", "203.0.113.7", 0)).toBe("203.0.113.7");
  });

  it("falls back to the socket address when the list is shorter than the offset (never trusts a spoofable leftmost)", () => {
    // Only one entry but 1 hop configured → offset underflows → socket, NOT the
    // lone (client-supplied) value.
    expect(extractClientIp("1.1.1.1", "203.0.113.7", 1)).toBe("203.0.113.7");
  });

  it("returns 'unknown' when neither XFF nor socket address is available", () => {
    expect(extractClientIp(undefined, undefined, 0)).toBe("unknown");
  });

  it("handles a header array (Node may surface XFF as string[])", () => {
    expect(extractClientIp(["1.1.1.1", "8.8.8.8"], "127.0.0.1", 0)).toBe(
      "8.8.8.8",
    );
  });

  it("trims whitespace and skips empty entries", () => {
    expect(extractClientIp("1.1.1.1 ,  , 8.8.8.8 ", "127.0.0.1", 0)).toBe(
      "8.8.8.8",
    );
  });
});
