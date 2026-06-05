import { describe, expect, it } from "vitest";
import { SingleFlight } from "../single-flight";

describe("SingleFlight (in-flight coalescing — threat model A3)", () => {
  it("coalesces concurrent calls for the same key into one fn() invocation", async () => {
    const sf = new SingleFlight<number>();
    let calls = 0;
    let resolveFn: (v: number) => void = () => {};
    const fn = () => {
      calls += 1;
      return new Promise<number>((r) => {
        resolveFn = r;
      });
    };
    const p1 = sf.run("k", fn);
    const p2 = sf.run("k", fn);
    const p3 = sf.run("k", fn);
    expect(calls).toBe(1); // only the first call ran fn()
    expect(sf.size()).toBe(1);
    resolveFn(42);
    expect(await Promise.all([p1, p2, p3])).toEqual([42, 42, 42]);
    expect(sf.size()).toBe(0); // released on settle
  });

  it("runs distinct keys independently", async () => {
    const sf = new SingleFlight<string>();
    let calls = 0;
    const run = (v: string) => {
      calls += 1;
      return Promise.resolve(v);
    };
    const [a, b] = await Promise.all([
      sf.run("a", () => run("a")),
      sf.run("b", () => run("b")),
    ]);
    expect([a, b]).toEqual(["a", "b"]);
    expect(calls).toBe(2);
  });

  it("releases the key on failure so the next call retries (no wedge)", async () => {
    const sf = new SingleFlight<number>();
    let calls = 0;
    await expect(
      sf.run("k", () => {
        calls += 1;
        return Promise.reject(new Error("boom"));
      }),
    ).rejects.toThrow("boom");
    expect(sf.size()).toBe(0);
    await expect(
      sf.run("k", () => {
        calls += 1;
        return Promise.resolve(7);
      }),
    ).resolves.toBe(7);
    expect(calls).toBe(2);
  });

  it("starts a fresh flight after the previous one has settled", async () => {
    const sf = new SingleFlight<number>();
    let calls = 0;
    await sf.run("k", () => {
      calls += 1;
      return Promise.resolve(1);
    });
    await sf.run("k", () => {
      calls += 1;
      return Promise.resolve(2);
    });
    expect(calls).toBe(2);
  });
});
