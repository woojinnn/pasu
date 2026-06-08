import { describe, expect, it } from "vitest";

import { fieldsForTrigger, operatorsFor, valueKindForField } from "../field-catalog";

describe("field catalog", () => {
  it("includes base gloss fields for any trigger", () => {
    const paths = fieldsForTrigger({ kind: "any" }).map((f) => f.path);
    expect(paths).toContain("context.recipient"); // a well-known base field (수신자)
  });

  it("hides container fields the form can't compare (ref / record)", () => {
    const paths = fieldsForTrigger({ kind: "any" }).map((f) => f.path);
    expect(paths).not.toContain("context.amountDesired"); // record (희망 수량)
    expect(paths).not.toContain("context.tokenIn"); // ref (입력 토큰)
    expect(paths).toContain("context.venue.name"); // its String subfield stays (베뉴 이름)
  });

  it("includes a custom enrichment field only for its applicable action", () => {
    const swap = fieldsForTrigger({ kind: "actionEq", entityType: "Amm::Action", id: "Swap" });
    expect(swap.map((f) => f.path)).toContain("context.custom.inputUsd"); // appliesTo: ["swap"]

    const transfer = fieldsForTrigger({ kind: "actionEq", entityType: "Erc20::Action", id: "Transfer" });
    expect(transfer.map((f) => f.path)).not.toContain("context.custom.inputUsd");
  });

  it("tags custom fields with source + the right kind", () => {
    const inputUsd = fieldsForTrigger({ kind: "actionEq", entityType: "Amm::Action", id: "Swap" }).find(
      (f) => f.path === "context.custom.inputUsd",
    )!;
    expect(inputUsd.source).toBe("custom");
    expect(inputUsd.fieldKind).toBe("primitive.decimal");
  });

  it("offers operators by field kind", () => {
    expect(operatorsFor("primitive.Bool")).toEqual(["==", "!="]);
    expect(operatorsFor("primitive.Long")).toContain(">=");
    expect(operatorsFor("primitive.decimal")).toContain("<");
    expect(operatorsFor("primitive.String")).toContain("in");
    expect(operatorsFor("record")).toEqual([]);
  });

  it("maps field kind to a value-widget kind", () => {
    expect(valueKindForField("primitive.Bool")).toBe("bool");
    expect(valueKindForField("primitive.decimal")).toBe("decimal");
    expect(valueKindForField("ref")).toBe("string");
  });
});
