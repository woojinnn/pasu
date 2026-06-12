import { useMemo, useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";

import {
  createListing,
  type CreateListingBody,
  type ListingKind,
  type MarketSeverity,
  type SetMember,
} from "../../server-api";
import { severityFromCedar } from "./policy-meta";
import { textToBlocks } from "../../cedar";
import { computeShippedHoles, manifestWithHoles } from "./publish-holes";
import { PublishPreviewTree } from "./PublishPreviewTree";
import {
  addressFieldRefs,
  extractHoles,
  redactCedar,
  type PublishHole,
} from "./publish-redact";

import "../market.css";

/** Per-kind input the editor passes in. The modal reads it as-is and asks
 *  the user for the marketplace metadata (slug, displayName, domain, etc.). */
export type PublishSource =
  | {
      kind: "policy";
      cedarText: string;
      manifest?: unknown;
      policyTree?: string | null;
      suggestedDisplayName: string;
      suggestedSlug: string;
    }
  | {
      kind: "package";
      suggestedDisplayName: string;
      suggestedSlug: string;
      description?: string;
      /** 사전 렌더된 멤버(defaults.packageId 기준 defs → cedar 텍스트). */
      members: readonly { slug: string; title: string; cedarText: string; manifest?: unknown }[];
    };

export interface PublishModalProps {
  open: boolean;
  onClose: () => void;
  source: PublishSource | null;
}

const SEMVER = "1.0.0";
const SLUG_RE = /^[A-Za-z0-9_./()-]{1,128}$/;

/** A policy being published, with its de-identification analysis. */
interface PublishRule {
  ruleId: string;
  title: string;
  cedarText: string;
  manifest?: unknown;
  holes: PublishHole[];
  refs: ReturnType<typeof addressFieldRefs>;
}

/**
 * Publish wizard — "마켓에 올리기". Two working steps:
 *   1. 비식별 확인 — address identifiers are ALWAYS blanked into parameter
 *      holes; numeric thresholds may be kept (추천값) or blanked.
 *   2. 이름·설명 — name + description, then publish.
 * The published Cedar carries no real addresses; the installer fills holes.
 */
export function PublishModal({ open, onClose, source }: PublishModalProps) {
  const navigate = useNavigate();
  const qc = useQueryClient();
  const [step, setStep] = useState<1 | 2>(1);
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  // Hole keys the author chose to KEEP public (주소 공개 / 숫자 추천값 남기기).
  // Default = all blanked. 주소를 남기는 건 "특정 주소로 거래되면 차단"처럼
  // 주소가 정책의 본질인 경우 — 공개한 값은 마켓에 그대로 노출된다.
  const [kept, setKept] = useState<Set<string>>(new Set());

  const rules = useMemo<PublishRule[]>(() => {
    if (!source) return [];
    if (source.kind === "policy") {
      const holes = extractHoles(source.cedarText);
      return [
        {
          ruleId: ruleIdOf(source.cedarText) || source.suggestedSlug,
          title: source.suggestedDisplayName,
          cedarText: source.cedarText,
          manifest: source.manifest,
          holes,
          refs: addressFieldRefs(source.cedarText, new Set(holes.map((h) => h.path))),
        },
      ];
    }
    const out: PublishRule[] = [];
    for (const m of source.members) {
      const holes = extractHoles(m.cedarText);
      out.push({
        ruleId: ruleIdOf(m.cedarText) || m.slug,
        title: m.title,
        cedarText: m.cedarText,
        manifest: m.manifest,
        holes,
        refs: addressFieldRefs(m.cedarText, new Set(holes.map((h) => h.path))),
      });
    }
    return out;
  }, [source]);

  // Aggregate counts for chips + summary.
  const numberHoles = useMemo(
    () => rules.flatMap((r) => r.holes.filter((h) => h.kind === "number")),
    [rules],
  );
  const addressHoles = useMemo(
    () => rules.flatMap((r) => r.holes.filter((h) => h.kind === "address")),
    [rules],
  );
  const keptNumCount = numberHoles.filter((h) => kept.has(h.key)).length;
  const keptAddrCount = addressHoles.filter((h) => kept.has(h.key)).length;
  // 비식별로 나가는 주소 칸 = 안 남긴 주소 hole. 런타임 비교(refs)는 가릴
  // 값 자체가 없으므로 "비움" 카운트에 넣지 않는다(안내 행으로만 표시).
  const blankedAddrCount = addressHoles.length - keptAddrCount;

  const reset = () => {
    setStep(1);
    setName("");
    setDescription("");
    setKept(new Set());
  };
  const close = () => {
    reset();
    onClose();
  };

  const toggleKeep = (key: string) =>
    setKept((prev) => {
      const n = new Set(prev);
      if (n.has(key)) n.delete(key);
      else n.add(key);
      return n;
    });

  const publishMut = useMutation({
    mutationFn: async (): Promise<{ slug: string; kind: ListingKind }> => {
      if (!source) throw new Error("no source");
      const slug = source.suggestedSlug.trim();
      const trimName = name.trim() || source.suggestedDisplayName;
      if (!SLUG_RE.test(slug)) {
        throw new Error("슬러그 형식이 잘못됐습니다 (영문/숫자/_.-()/ 만, 1-128자)");
      }
      const desc = description.trim()
        ? { en: description.trim(), ko: description.trim() }
        : undefined;

      // 블랭킹이 적용된 hole(공개로 남기지 않은 칸 전부)의 위치 기반 param
      // 이름을 계산해 manifest에 동봉 — 설치자가 어느 칸을 채워야 하는지의
      // 유일한 출처다 (redacted 텍스트엔 hole 흔적이 없다). 공개로 남긴 칸은
      // hole이 아니므로 여기서 빠지고, 설치 게이트도 적용되지 않는다.
      const blankedOf = (r: PublishRule) => r.holes.filter((h) => !kept.has(h.key));

      if (source.kind === "policy") {
        const r = rules[0];
        const cedar = r ? redactCedar(r.cedarText, r.holes, kept) : source.cedarText;
        const shipped = r ? await computeShippedHoles(cedar, blankedOf(r), textToBlocks) : null;
        const body: CreateListingBody = {
          slug,
          kind: "policy",
          display_name: { en: trimName, ko: trimName },
          description: desc,
          domain: "security",
          severity: (severityFromCedar(source.cedarText) === "deny"
            ? "deny"
            : "warn") as MarketSeverity,
          version: SEMVER,
          cedar_text: cedar,
          manifest: manifestWithHoles(source.manifest, shipped, r?.ruleId ?? slug),
          policy_tree: source.policyTree ?? undefined,
        };
        await createListing(body);
        return { slug, kind: "policy" };
      }

      const members: SetMember[] = [];
      for (const r of rules) {
        const cedar = redactCedar(r.cedarText, r.holes, kept);
        const shipped = await computeShippedHoles(cedar, blankedOf(r), textToBlocks);
        members.push({
          slug: r.ruleId,
          display_name: r.title,
          cedar_text: cedar,
          manifest: manifestWithHoles(r.manifest, shipped, r.ruleId),
        });
      }
      if (members.length === 0) throw new Error("발행할 멤버 정책이 없습니다.");
      const body: CreateListingBody = {
        slug,
        kind: "set",
        display_name: { en: trimName, ko: trimName },
        description: desc,
        version: SEMVER,
        members,
      };
      await createListing(body);
      return { slug, kind: "set" };
    },
    onSuccess: async ({ slug }) => {
      await qc.invalidateQueries({ queryKey: ["market-listings"] });
      close();
      navigate(`/market/${encodeURIComponent(slug)}`);
    },
  });

  if (!open || !source) return null;

  const seededName = name || source.suggestedDisplayName;
  const loadingMembers = false;

  return (
    <div
      className="pub-backdrop"
      onClick={(e) => {
        if (e.target === e.currentTarget && !publishMut.isPending) close();
      }}
    >
      <div className="pub-modal" role="dialog" aria-modal>
        <header className="pub-head">
          <span className="pub-head-ic"><ShieldIcon /></span>
          <div className="pub-head-t">
            <div className="pub-title">마켓에 올리기</div>
            <div className="pub-sub">
              내가 큐레이션한 패키지를 공개해 다른 사용자가 담을 수 있게 합니다.
            </div>
          </div>
          <button type="button" className="pub-x" onClick={close} aria-label="닫기">
            <XIcon />
          </button>
        </header>

        <Stepper step={step} />

        <div className="pub-body">
          {step === 1 ? (
            <Step1
              rules={rules}
              blankedAddrCount={blankedAddrCount}
              keptAddrCount={keptAddrCount}
              numberCount={numberHoles.length}
              kept={kept}
              onToggleKeep={toggleKeep}
              loading={loadingMembers}
            />
          ) : (
            <Step2
              name={seededName}
              onName={setName}
              description={description}
              onDescription={setDescription}
              ruleCount={rules.length}
              blankedAddrCount={blankedAddrCount}
              keptAddrCount={keptAddrCount}
              keptNumCount={keptNumCount}
              numberCount={numberHoles.length}
            />
          )}

          {publishMut.isError && (
            <div className="pub-error">{(publishMut.error as Error).message}</div>
          )}
        </div>

        <footer className="pub-foot">
          {step === 1 ? (
            <>
              {keptAddrCount > 0 ? (
                <span className="pub-foot-note warn">
                  주소 {keptAddrCount}칸이 마켓에 공개로 올라갑니다
                </span>
              ) : (
                <span className="pub-foot-note">
                  주소류는 기본 비워집니다 · 칸별로 공개 선택 가능
                </span>
              )}
              <button type="button" className="pub-btn ghost" onClick={close}>
                취소
              </button>
              <button
                type="button"
                className="pub-btn primary"
                onClick={() => setStep(2)}
                disabled={loadingMembers}
              >
                다음 ›
              </button>
            </>
          ) : (
            <>
              <button type="button" className="pub-btn ghost" onClick={() => setStep(1)}>
                ‹ 뒤로
              </button>
              <span className="pub-spc" />
              <button
                type="button"
                className="pub-btn publish"
                onClick={() => publishMut.mutate()}
                disabled={publishMut.isPending}
              >
                <ShieldIcon />
                {publishMut.isPending ? "공개 중…" : "마켓에 공개"}
              </button>
            </>
          )}
        </footer>
      </div>
    </div>
  );
}

/* ── stepper ───────────────────────────────────────────────────────── */
function Stepper({ step }: { step: 1 | 2 }) {
  const steps = [
    { n: 1, label: "비식별 확인" },
    { n: 2, label: "이름·설명" },
    { n: 3, label: "공개" },
  ];
  return (
    <div className="pub-stepper">
      {steps.map((s, i) => (
        <div key={s.n} className="pub-step-wrap">
          <div
            className={`pub-step${step === s.n ? " on" : ""}${step > s.n ? " done" : ""}`}
          >
            <span className="pub-step-n">{step > s.n ? "✓" : s.n}</span>
            <span className="pub-step-l">{s.label}</span>
          </div>
          {i < steps.length - 1 && <span className="pub-step-line" />}
        </div>
      ))}
    </div>
  );
}

/* ── step 1: de-identification ─────────────────────────────────────── */
function Step1(props: {
  rules: PublishRule[];
  blankedAddrCount: number;
  keptAddrCount: number;
  numberCount: number;
  kept: Set<string>;
  onToggleKeep: (key: string) => void;
  loading: boolean;
}) {
  const { rules, blankedAddrCount, keptAddrCount, numberCount, kept, onToggleKeep, loading } =
    props;
  const [openTrees, setOpenTrees] = useState<Set<string>>(new Set());
  const toggleTree = (ruleId: string) =>
    setOpenTrees((prev) => {
      const n = new Set(prev);
      if (n.has(ruleId)) n.delete(ruleId);
      else n.add(ruleId);
      return n;
    });
  if (loading) return <div className="pub-muted">멤버 정책 불러오는 중…</div>;

  return (
    <>
      <div className="pub-info">
        <LockIcon />
        <div>
          <b>개인정보 자동 비식별 (기본)</b>
          <div>
            주소류 식별자(지갑·수취인·위임 대상·allowlist)는 기본으로{" "}
            <b>파라미터 구멍으로 비워서</b> 올라가고, 담는 사람이 자기 값을 채웁니다.
            주소가 정책의 본질이면(예: 특정 주소로 보내면 차단) 칸별로{" "}
            <b>값 공개</b>를 선택할 수 있어요 — 공개한 값은 마켓에 그대로 노출됩니다.
          </div>
        </div>
      </div>

      <div className="pub-chips">
        <span className="pub-chip">
          <SearchIcon /> 주소류 (기본 비움) · {blankedAddrCount}
        </span>
        {keptAddrCount > 0 && (
          <span className="pub-chip warn"># 주소 공개 · {keptAddrCount}</span>
        )}
        <span className="pub-chip">
          # 숫자 임계값 (선택) · {numberCount}
        </span>
      </div>

      <div className="pub-rules">
        {rules.map((r) => (
          <div key={r.ruleId} className="pub-rule">
            <div className="pub-rule-head">
              <span className="pub-rule-dot" />
              <span className="pub-rule-title">{r.title}</span>
              <span className="pub-rule-id">{r.ruleId}</span>
              <button
                type="button"
                className={`pub-tree-toggle${openTrees.has(r.ruleId) ? " on" : ""}`}
                onClick={() => toggleTree(r.ruleId)}
              >
                조건 보기
              </button>
            </div>

            {openTrees.has(r.ruleId) && (
              <PublishPreviewTree
                cedarText={r.cedarText}
                holes={r.holes}
                kept={kept}
                onToggleKeep={onToggleKeep}
              />
            )}

            {r.refs.map((ref) => (
              <div key={ref.path} className="pub-field">
                <span className="pub-field-ic addr"><SearchIcon /></span>
                <div className="pub-field-main">
                  <div className="pub-field-label">
                    {ref.label} <code>{ref.path}</code>
                  </div>
                  <div className="pub-field-val">
                    <span className="pub-runtime">
                      런타임 값끼리 비교해요 — 텍스트에 가릴 개인 값이 없어요
                    </span>
                  </div>
                </div>
                <span className="pub-blanked">개인값 없음</span>
              </div>
            ))}

            {r.holes.map((h) =>
              h.kind === "address" ? (
                <div key={h.key} className={`pub-field${kept.has(h.key) ? " kept" : ""}`}>
                  <span className="pub-field-ic addr"><SearchIcon /></span>
                  <div className="pub-field-main">
                    <div className="pub-field-label">
                      {h.label} <code>{h.path}</code>
                    </div>
                    <div className="pub-field-val">
                      {kept.has(h.key) ? (
                        <>
                          <span>{h.display}</span>
                          <span className="arrow">→</span>
                          <span className="param public">마켓에 공개</span>
                        </>
                      ) : (
                        <>
                          <span className="redacted">{h.display}</span>
                          <span className="arrow">→</span>
                          <span className="param">{h.paramName}</span>
                        </>
                      )}
                    </div>
                    {(h.addrCount ?? 0) > 1 && (
                      <div className="pub-field-sub mono" title={addrsOf(h.raw).join("\n")}>
                        {addrsOf(h.raw).map(shortAddr).join(" · ")}
                      </div>
                    )}
                  </div>
                  <div className="pub-numtoggle pub-addrtoggle">
                    <button
                      type="button"
                      className={!kept.has(h.key) ? "on" : ""}
                      onClick={() => kept.has(h.key) && onToggleKeep(h.key)}
                    >
                      비우기
                      <small>{h.paramName}</small>
                    </button>
                    <button
                      type="button"
                      className={kept.has(h.key) ? "on public" : ""}
                      onClick={() => !kept.has(h.key) && onToggleKeep(h.key)}
                    >
                      값 공개
                      <small>{h.display}</small>
                    </button>
                  </div>
                </div>
              ) : (
                <div key={h.key} className="pub-field">
                  <span className="pub-field-ic num">#</span>
                  <div className="pub-field-main">
                    <div className="pub-field-label">
                      {h.label} <code>{h.path}</code>
                    </div>
                    <div className="pub-field-sub">
                      원작자가 쓴 값 <b>{h.display}{h.unit ?? ""}</b>
                    </div>
                  </div>
                  <div className="pub-numtoggle">
                    <button
                      type="button"
                      className={!kept.has(h.key) ? "on" : ""}
                      onClick={() => kept.has(h.key) && onToggleKeep(h.key)}
                    >
                      비우기
                      <small>{h.paramName}</small>
                    </button>
                    <button
                      type="button"
                      className={kept.has(h.key) ? "on" : ""}
                      onClick={() => !kept.has(h.key) && onToggleKeep(h.key)}
                    >
                      추천값 남기기
                      <small>{h.display}{h.unit ?? ""}</small>
                    </button>
                  </div>
                </div>
              ),
            )}

            {r.holes.length === 0 && r.refs.length === 0 && (
              <div className="pub-rule-clean">비식별할 식별자가 없어요.</div>
            )}
          </div>
        ))}
      </div>
    </>
  );
}

/* ── step 2: name + description + summary ──────────────────────────── */
function Step2(props: {
  name: string;
  onName: (v: string) => void;
  description: string;
  onDescription: (v: string) => void;
  ruleCount: number;
  blankedAddrCount: number;
  keptAddrCount: number;
  keptNumCount: number;
  numberCount: number;
}) {
  const {
    name,
    onName,
    description,
    onDescription,
    ruleCount,
    blankedAddrCount,
    keptAddrCount,
    keptNumCount,
    numberCount,
  } = props;
  return (
    <>
      <label className="pub-l">패키지 이름</label>
      <input
        className="pub-input"
        value={name}
        onChange={(e) => onName(e.target.value)}
        maxLength={120}
      />

      <label className="pub-l">설명</label>
      <textarea
        className="pub-textarea"
        value={description}
        onChange={(e) => onDescription(e.target.value)}
        rows={3}
        maxLength={500}
        placeholder="이 패키지가 무엇을 막아주는지 간단히 적어주세요"
      />

      <div className="pub-summary">
        <div className="pub-summary-t">공개될 내용</div>
        <div className="pub-summary-row">
          <span>정책 수</span>
          <b>{ruleCount}개</b>
        </div>
        <div className="pub-summary-row">
          <span>주소 구멍(비식별)</span>
          <b>{blankedAddrCount}</b>
        </div>
        {keptAddrCount > 0 && (
          <div className="pub-summary-row warn">
            <span>주소 공개</span>
            <b>{keptAddrCount}</b>
          </div>
        )}
        <div className="pub-summary-row">
          <span>추천값 남김</span>
          <b>
            {keptNumCount} / {numberCount}
          </b>
        </div>
      </div>

      <div className="pub-note">
        <ShieldIcon /> 공개 = 누구나 마켓에서 담을 수 있음. 비공개로 되돌릴 수 있어요.
      </div>
    </>
  );
}

/* ── helpers ───────────────────────────────────────────────────────── */
function ruleIdOf(cedarText: string): string {
  const m = cedarText.match(/@id\(\s*"([^"]+)"\s*\)/);
  return m ? m[1] : "";
}

function addrsOf(raw: string): string[] {
  return raw.match(/0x[0-9a-fA-F]{40}/g) ?? [];
}

function shortAddr(a: string): string {
  return `${a.slice(0, 6)}…${a.slice(-4)}`;
}

/* ── icons ─────────────────────────────────────────────────────────── */
const stroke = {
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 1.8,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
};
function ShieldIcon() {
  return (
    <svg viewBox="0 0 24 24" width="16" height="16" {...stroke}>
      <path d="M12 3l7 3v5c0 4.5-3 7.5-7 9-4-1.5-7-4.5-7-9V6z" />
      <path d="m9 12 2 2 4-4" />
    </svg>
  );
}
function LockIcon() {
  return (
    <svg viewBox="0 0 24 24" width="14" height="14" {...stroke}>
      <rect x="5" y="11" width="14" height="9" rx="2" />
      <path d="M8 11V8a4 4 0 0 1 8 0v3" />
    </svg>
  );
}
function SearchIcon() {
  return (
    <svg viewBox="0 0 24 24" width="14" height="14" {...stroke}>
      <circle cx="11" cy="11" r="7" />
      <path d="m20 20-3.5-3.5" />
    </svg>
  );
}
function XIcon() {
  return (
    <svg viewBox="0 0 24 24" width="16" height="16" {...stroke}>
      <path d="M6 6l12 12M18 6 6 18" />
    </svg>
  );
}
