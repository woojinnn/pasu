import { describe, expect, it } from "vitest";

import { formToIr } from "../../../cedar/form";
import type { FormModel } from "../../../cedar/form";
import {
  MANIFEST_HOLES_KEY,
  computeShippedHoles,
  manifestWithHoles,
  splitManifestHoles,
} from "../publish-holes";
import { ZERO_ADDR, type PublishHole } from "../publish-redact";

/** redact 직후의 정책 모델 — 주소는 제로주소, decimal 임곗값은 "0.0". */
const REDACTED: FormModel = {
  trigger: { kind: "actionEq", entityType: "Pasu::Action", id: "swap" },
  when: [
    {
      joiner: "and",
      fieldPath: "context.recipient",
      op: "==",
      value: { kind: "string", value: ZERO_ADDR },
    },
    {
      joiner: "and",
      fieldPath: "context.custom.inputUsd",
      op: ">=",
      value: { kind: "decimal", value: "0.0" },
    },
    {
      joiner: "and",
      fieldPath: "context.slippageBp",
      op: ">=",
      value: { kind: "long", value: 150 }, // 작성자가 남긴 추천값 — hole 아님
    },
  ],
  unless: [],
  id: "p",
  severity: "warn",
  reason: "",
};

const hole = (path: string, kind: PublishHole["kind"], label: string): PublishHole => ({
  key: `p#${path}`,
  ruleId: "p",
  kind,
  path,
  label,
  paramName: `?${path.split(".").pop()}`,
  display: "",
  raw: "",
});

const toBlocks = async () => [formToIr(REDACTED)];

describe("computeShippedHoles", () => {
  it("maps blanked holes to positional param names in form order", async () => {
    const shipped = await computeShippedHoles(
      "(unused — toBlocks is injected)",
      [
        hole("context.recipient", "address", "받는 주소"),
        hole("context.custom.inputUsd", "number", "들어가는 금액(USD)"),
      ],
      toBlocks,
    );
    expect(shipped).toEqual([
      { name: "v1", type: "address", label: "받는 주소", required: true },
      { name: "v2", type: "decimal", label: "들어가는 금액(USD)", required: true },
    ]);
  });

  it("skips holes whose leaf is not in placeholder state (kept 추천값)", async () => {
    const shipped = await computeShippedHoles(
      "x",
      [hole("context.slippageBp", "number", "슬리피지")],
      toBlocks,
    );
    expect(shipped).toEqual([]); // 150은 플레이스홀더(0)가 아니다
  });

  it("returns null for form-incompatible policies", async () => {
    const shipped = await computeShippedHoles(
      "x",
      [hole("context.recipient", "address", "받는 주소")],
      async () => {
        throw new Error("parse fail");
      },
    );
    expect(shipped).toBeNull();
  });
});

describe("manifestWithHoles / splitManifestHoles", () => {
  const SHIPPED = [
    { name: "v1", type: "address" as const, label: "받는 주소", required: true as const },
  ];

  it("synthesizes a valid minimal ManifestV2 when the policy had none", () => {
    const m = manifestWithHoles(undefined, SHIPPED, "my-rule") as Record<string, unknown>;
    expect(m.id).toBe("my-rule");
    expect(m.schema_version).toBe(2);
    expect(m[MANIFEST_HOLES_KEY]).toEqual(SHIPPED);
  });

  it("round-trips: split removes the key and returns the specs", () => {
    const m = manifestWithHoles({ id: "x", schema_version: 2, trigger: { scope: "inner" } }, SHIPPED, "x");
    const { shipped, manifest } = splitManifestHoles(m);
    expect(shipped).toEqual(SHIPPED);
    expect(manifest).toEqual({ id: "x", schema_version: 2, trigger: { scope: "inner" } });
  });

  it("no holes → manifest untouched", () => {
    expect(manifestWithHoles(undefined, [], "x")).toBeUndefined();
    expect(manifestWithHoles(undefined, null, "x")).toBeUndefined();
    expect(splitManifestHoles(undefined)).toEqual({ shipped: [], manifest: undefined });
  });
});
