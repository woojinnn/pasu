import { describe, expect, it, vi } from "vitest";
import { listingToDefs } from "./market-install-convert";
import type { PolicyIR } from "../cedar/blocks";

const ir = { kind: "policy" } as unknown as PolicyIR;
const textToBlocks = vi.fn(async (t: string) => (t.includes("bad") ? [] : [ir]));

const policyVersion = { cedar_text: "permit(...);", manifest: { id: "m" }, members: [] };
const setVersion = {
  cedar_text: "",
  manifest: undefined,
  members: [
    { slug: "a", cedar_text: "permit(a);", manifest: { id: "a" } },
    { slug: "b", cedar_text: "permit(b);", manifest: { id: "b" } },
  ],
};

describe("listingToDefs", () => {
  it("policy listing → 1 def with market id/source/listing refs", async () => {
    const defs = await listingToDefs(
      { id: "L1", kind: "policy", displayName: "한도", version: "1.2.0", cat: "스왑" },
      policyVersion as never,
      textToBlocks,
    );
    expect(defs).toHaveLength(1);
    expect(defs[0].id).toBe("def::market.L1");
    expect(defs[0]).toMatchObject({
      source: "market",
      sourceListingId: "L1",
      sourceVersion: "1.2.0",
      displayName: "한도",
      cat: "스왑",
      holes: [],
    });
    expect(defs[0].skeleton).toEqual({ ir, manifest: { id: "m" } });
  });

  it("set listing → member defs with per-member ids", async () => {
    const defs = await listingToDefs(
      { id: "L2", kind: "set", displayName: "팩", version: "1.0.0", cat: undefined },
      setVersion as never,
      textToBlocks,
    );
    expect(defs.map((d) => d.id)).toEqual(["def::market.L2.a", "def::market.L2.b"]);
    expect(defs.every((d) => d.sourceListingId === "L2")).toBe(true);
  });

  it("unconvertible cedar aborts the whole install with the member name", async () => {
    await expect(
      listingToDefs(
        { id: "L3", kind: "set", displayName: "팩", version: "1", cat: undefined },
        { cedar_text: "", members: [{ slug: "x", cedar_text: "bad", manifest: {} }] } as never,
        textToBlocks,
      ),
    ).rejects.toThrow(/x/);
  });
});
