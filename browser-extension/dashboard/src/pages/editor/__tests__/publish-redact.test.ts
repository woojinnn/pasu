import { describe, expect, it } from "vitest";

import { extractHoles, redactCedar } from "../publish-redact";

const RECIPIENT = `@id("recipient-blocklist-deny")
forbid(principal, action, resource)
when { context.recipient == "0xA1c4000000000000000000000000000000007e29" };`;

const ALLOWLIST = `@id("gov-delegatee-allowlist-deny")
forbid(principal, action, resource)
when { ["0x91d2000000000000000000000000000000000001", "0x44ab000000000000000000000000000000000002"].contains(context.delegatee) };`;

const SLIPPAGE = `@id("swap-slippage-wide-warn")
forbid(principal, action, resource)
when { context.slippageBp >= 150 };`;

describe("extractHoles", () => {
  it("finds a literal address comparison and blanks it (forced)", () => {
    const holes = extractHoles(RECIPIENT);
    const addr = holes.find((h) => h.kind === "address");
    expect(addr).toBeTruthy();
    expect(addr!.path).toBe("context.recipient");
    expect(addr!.paramName).toBe("?recipient");
    const out = redactCedar(RECIPIENT, holes, new Set());
    expect(out).not.toContain("0xA1c4000000000000000000000000000000007e29");
  });

  it("finds an address set behind .contains()", () => {
    const holes = extractHoles(ALLOWLIST);
    const addr = holes.find((h) => h.kind === "address");
    expect(addr).toBeTruthy();
    expect(addr!.addrCount).toBe(2);
    expect(addr!.path).toBe("context.delegatee");
  });

  it("finds a numeric threshold with its value", () => {
    const holes = extractHoles(SLIPPAGE);
    const num = holes.find((h) => h.kind === "number");
    expect(num).toBeTruthy();
    expect(num!.display).toBe("150");
  });

  it("redacts addresses always; keeps a number only when chosen", () => {
    const holes = extractHoles(SLIPPAGE);
    const num = holes.find((h) => h.kind === "number")!;

    const kept = redactCedar(SLIPPAGE, holes, new Set([num.key]));
    expect(kept).toContain("150"); // author kept the recommended value

    const blanked = redactCedar(SLIPPAGE, holes, new Set());
    expect(blanked).not.toContain("150"); // blanked to 0
    expect(blanked).toContain("context.slippageBp >= 0");
  });

  it("blanks a real address to the zero address", () => {
    const holes = extractHoles(ALLOWLIST);
    const out = redactCedar(ALLOWLIST, holes, new Set());
    expect(out).not.toContain("0x91d2000000000000000000000000000000000001");
  });

  it("finds an address literal inside attr.contains(...) (form contains/notContains)", () => {
    const cedar = `@id("p")
forbid(principal, action, resource)
when { !(context.path.contains("0x7a3f000000000000000000000000000000009c21")) };`;
    const holes = extractHoles(cedar);
    const addr = holes.find((h) => h.kind === "address");
    expect(addr).toBeTruthy();
    expect(addr!.path).toBe("context.path");
    const out = redactCedar(cedar, holes, new Set());
    expect(out).not.toContain("0x7a3f000000000000000000000000000000009c21");
  });

  it("finds an address literal on the LEFT of ==", () => {
    const cedar = `@id("p")
forbid(principal, action, resource)
when { "0x7a3f000000000000000000000000000000009c21" == context.recipient };`;
    const holes = extractHoles(cedar);
    expect(holes.find((h) => h.kind === "address")?.path).toBe("context.recipient");
  });

  it("finds a decimal threshold in extension-method form and blanks it to a VALID decimal", () => {
    const cedar = `@id("p")
forbid(principal, action, resource)
when { context.amountUsd.greaterThanOrEqual(decimal("3.0")) };`;
    const holes = extractHoles(cedar);
    const num = holes.find((h) => h.kind === "number");
    expect(num).toBeTruthy();
    expect(num!.display).toBe("3.0");
    const out = redactCedar(cedar, holes, new Set());
    expect(out).toContain('decimal("0.0")'); // decimal("0")은 Cedar가 거부한다
    expect(out).not.toContain('decimal("3.0")');
  });

  it("replaces EVERY occurrence of a repeated literal", () => {
    const cedar = `@id("p")
forbid(principal, action, resource)
when { context.recipient == "0xA1c4000000000000000000000000000000007e29"
  || context.sender == "0xA1c4000000000000000000000000000000007e29" };`;
    const holes = extractHoles(cedar);
    const out = redactCedar(cedar, holes, new Set());
    expect(out).not.toContain("0xA1c4000000000000000000000000000000007e29");
  });

  // wasm EST→text 렌더러는 피연산자를 괄호로 감싼다 — 폼으로 만든 정책이
  // 게시될 때의 실제 모양. 탐지는 이 모양도 잡아야 한다.
  describe("괄호로 감싼 렌더 출력", () => {
    it("(path) == 리터럴", () => {
      const cedar = `@id("p")
forbid(principal, action, resource)
when { ((context.recipient) == "0xA1c4000000000000000000000000000000007e29") };`;
      const holes = extractHoles(cedar);
      expect(holes.find((h) => h.kind === "address")?.path).toBe("context.recipient");
    });

    it("(셋 리터럴).contains((path))", () => {
      const cedar = `@id("p")
forbid(principal, action, resource)
when { (["0x91d2000000000000000000000000000000000001"]).contains((context.delegatee)) };`;
      const holes = extractHoles(cedar);
      expect(holes.find((h) => h.kind === "address")?.path).toBe("context.delegatee");
    });

    it("(path).contains(리터럴)", () => {
      const cedar = `@id("p")
forbid(principal, action, resource)
when { ((context.path).contains("0x7a3f000000000000000000000000000000009c21")) };`;
      expect(extractHoles(cedar).find((h) => h.kind === "address")?.path).toBe("context.path");
    });

    it("(path).greaterThan(decimal(...)) — 폼의 3달러 임곗값", () => {
      const cedar = `@id("new-form-test")
forbid(principal, action, resource)
when { ((context.custom.inputUsd).greaterThan(decimal("3.0"))) };`;
      const holes = extractHoles(cedar);
      const num = holes.find((h) => h.kind === "number");
      expect(num?.path).toBe("context.custom.inputUsd");
      expect(num?.display).toBe("3.0");
    });

    it("((path)) >= 숫자", () => {
      const cedar = `@id("p")
forbid(principal, action, resource)
when { (((context.slippageBp)) >= 150) };`;
      expect(extractHoles(cedar).find((h) => h.kind === "number")?.display).toBe("150");
    });

    it("중첩 경로의 내부 괄호 — ((context.custom).inputUsd).greaterThan (wasm 실측 모양)", () => {
      // wasm est_json_to_policy_text 라운드트립 실측: getAttr마다 수신자가
      // 괄호로 감싸인다. path는 점 표기로 정규화돼 나와야 gloss/폼 leaf와 맞는다.
      const cedar = `@id("new-form-test")
forbid(principal, action, resource)
when { ((context.custom).inputUsd).greaterThan(decimal("3.0")) };`;
      const num = extractHoles(cedar).find((h) => h.kind === "number");
      expect(num?.path).toBe("context.custom.inputUsd");
      expect(num?.display).toBe("3.0");
    });

    it("중첩 경로의 내부 괄호 — 주소 비교", () => {
      const cedar = `@id("p")
forbid(principal, action, resource)
when { (((context.custom).counterparty) == "0xA1c4000000000000000000000000000000007e29") };`;
      const addr = extractHoles(cedar).find((h) => h.kind === "address");
      expect(addr?.path).toBe("context.custom.counterparty");
    });

    it("리터럴 == (path)", () => {
      const cedar = `@id("p")
forbid(principal, action, resource)
when { ("0xA1c4000000000000000000000000000000007e29" == (context.recipient)) };`;
      expect(extractHoles(cedar).find((h) => h.kind === "address")?.path).toBe("context.recipient");
    });
  });

  it("센티널 주소는 개인 값이 아니다 — uint160::MAX 비교는 hole로 안 잡힌다", () => {
    const cedar = `@id("unlimited-approval-deny")
forbid(principal, action, resource)
when { context.amount == "0xffffffffffffffffffffffffffffffffffffffff" };`;
    expect(extractHoles(cedar)).toEqual([]);
  });

  it("센티널 주소 — 소각주소(제로/dead) 집합 비교도 hole로 안 잡힌다", () => {
    const cedar = `@id("send-first-time-or-burn-recipient-warn")
forbid(principal, action, resource)
when { ["0x0000000000000000000000000000000000000000",
   "0x000000000000000000000000000000000000dead"].contains(context.recipient) };`;
    expect(extractHoles(cedar)).toEqual([]);
  });

  it("센티널 + 개인 주소가 섞인 집합은 여전히 hole (개인 값을 가려야 함)", () => {
    const cedar = `@id("p")
forbid(principal, action, resource)
when { ["0x0000000000000000000000000000000000000000",
   "0x91d2000000000000000000000000000000000001"].contains(context.recipient) };`;
    const holes = extractHoles(cedar);
    expect(holes).toHaveLength(1);
    expect(holes[0]!.kind).toBe("address");
  });

  it("keeps an address the author chose to publish (마스킹 opt-out)", () => {
    const holes = extractHoles(RECIPIENT);
    const addr = holes.find((h) => h.kind === "address")!;
    const out = redactCedar(RECIPIENT, holes, new Set([addr.key]));
    expect(out).toContain("0xA1c4000000000000000000000000000000007e29");
  });

  it("keeps an address SET literal when chosen", () => {
    const holes = extractHoles(ALLOWLIST);
    const addr = holes.find((h) => h.kind === "address")!;
    const out = redactCedar(ALLOWLIST, holes, new Set([addr.key]));
    expect(out).toContain("0x91d2000000000000000000000000000000000001");
    expect(out).toContain("0x44ab000000000000000000000000000000000002");
  });

  it("does not mangle a longer number that contains the blanked one as a substring", () => {
    const cedar = `@id("p")
forbid(principal, action, resource)
when { context.slippageBp >= 150 && context.other == decimal("150.5") };`;
    const holes = extractHoles(cedar);
    const bare = holes.find((h) => h.raw === "150")!;
    const out = redactCedar(cedar, [bare], new Set());
    expect(out).toContain("context.slippageBp >= 0");
    expect(out).toContain('decimal("150.5")');
  });
});
