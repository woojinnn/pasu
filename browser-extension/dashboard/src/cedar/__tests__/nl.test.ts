import { describe, expect, it } from "vitest";

import { naturalCondition, withJosa } from "../nl";

describe("naturalCondition", () => {
  it("picks the right Korean particle (받침)", () => {
    expect(withJosa("수신자", "이", "가")).toBe("수신자가"); // no 받침 → 가
    expect(withJosa("대상", "이", "가")).toBe("대상이"); // ㅇ 받침 → 이
    expect(withJosa("LP 토큰", "이", "가")).toBe("LP 토큰이"); // ㄴ 받침 → 이
    expect(withJosa("1", "이", "가")).toBe("1이"); // 일 → ㄹ 받침 → 이
  });

  it("renders comparisons as sentences", () => {
    expect(naturalCondition({ subject: "수신자", op: "==", value: "", emptyStr: true })).toBe(
      "수신자가 비어 있을 때",
    );
    expect(naturalCondition({ subject: "수신자", op: "!=", value: "내 지갑 주소" })).toBe(
      "수신자가 내 지갑 주소가 아닐 때",
    );
    expect(naturalCondition({ subject: "최대 레버리지", op: ">=", value: "10" })).toBe(
      "최대 레버리지가 10 이상일 때",
    );
    expect(naturalCondition({ subject: "대상", op: "in", value: "[a, b]" })).toBe(
      "대상이 [a, b] 중 하나일 때",
    );
  });

  it("handles negation", () => {
    expect(naturalCondition({ subject: "대상", op: "in", value: "[a]", neg: true })).toBe(
      "대상이 [a] 중 하나도 아닐 때",
    );
    expect(naturalCondition({ subject: "수신자", op: "==", value: "", emptyStr: true, neg: true })).toBe(
      "수신자가 비어 있지 않을 때",
    );
  });
});
