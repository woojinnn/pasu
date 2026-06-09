import { useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";

import {
  createListing,
  listManagedPolicies,
  type CreateListingBody,
  type ListingKind,
  type MarketSeverity,
  type SetMember,
} from "../../server-api";
import { severityFromCedar } from "./policy-meta";
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
      kind: "set";
      suggestedDisplayName: string;
      suggestedSlug: string;
      description?: string;
      /** Local dashboard:: ids of member policies. The modal looks them up
       *  in the SW list to snapshot cedar_text/manifest at publish time. */
      memberIds: readonly string[];
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
  const policiesQ = useQuery({
    queryKey: ["managed-policies"],
    queryFn: listManagedPolicies,
    enabled: open && source?.kind === "set",
  });

  const [step, setStep] = useState<1 | 2>(1);
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  // Number-hole keys the author chose to KEEP (추천값 남기기). Default = all blanked.
  const [keptNumbers, setKeptNumbers] = useState<Set<string>>(new Set());

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
    const byId = new Map((policiesQ.data ?? []).map((p) => [p.id, p]));
    const out: PublishRule[] = [];
    for (const mid of source.memberIds) {
      const p = byId.get(mid);
      if (!p) continue;
      const holes = extractHoles(p.text);
      out.push({
        ruleId: ruleIdOf(p.text) || stripPrefix(p.id),
        title: p.displayName ?? stripPrefix(p.id),
        cedarText: p.text,
        manifest: p.manifest,
        holes,
        refs: addressFieldRefs(p.text, new Set(holes.map((h) => h.path))),
      });
    }
    return out;
  }, [source, policiesQ.data]);

  // Aggregate counts for chips + summary.
  const numberHoles = useMemo(
    () => rules.flatMap((r) => r.holes.filter((h) => h.kind === "number")),
    [rules],
  );
  const addressCount = useMemo(
    () =>
      rules.reduce(
        (n, r) => n + r.holes.filter((h) => h.kind === "address").length + r.refs.length,
        0,
      ),
    [rules],
  );

  const reset = () => {
    setStep(1);
    setName("");
    setDescription("");
    setKeptNumbers(new Set());
  };
  const close = () => {
    reset();
    onClose();
  };

  const toggleKeep = (key: string) =>
    setKeptNumbers((prev) => {
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

      if (source.kind === "policy") {
        const r = rules[0];
        const cedar = r ? redactCedar(r.cedarText, r.holes, keptNumbers) : source.cedarText;
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
          manifest: source.manifest,
          policy_tree: source.policyTree ?? undefined,
        };
        await createListing(body);
        return { slug, kind: "policy" };
      }

      const members: SetMember[] = rules.map((r) => ({
        slug: r.ruleId,
        display_name: r.title,
        cedar_text: redactCedar(r.cedarText, r.holes, keptNumbers),
        manifest: r.manifest,
      }));
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
  const loadingMembers = source.kind === "set" && policiesQ.isLoading;

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
              addressCount={addressCount}
              numberCount={numberHoles.length}
              keptNumbers={keptNumbers}
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
              addressCount={addressCount}
              keptCount={keptNumbers.size}
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
              <span className="pub-foot-note">주소류는 항상 비워집니다 · 우회 불가</span>
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
  addressCount: number;
  numberCount: number;
  keptNumbers: Set<string>;
  onToggleKeep: (key: string) => void;
  loading: boolean;
}) {
  const { rules, addressCount, numberCount, keptNumbers, onToggleKeep, loading } = props;
  if (loading) return <div className="pub-muted">멤버 정책 불러오는 중…</div>;

  return (
    <>
      <div className="pub-info">
        <LockIcon />
        <div>
          <b>개인정보 자동 비식별 (강제)</b>
          <div>
            주소류 식별자(지갑·수취인·위임 대상·allowlist)는{" "}
            <b>무조건 파라미터 구멍으로 비워서</b> 올라갑니다. 담는 사람이 자기 값을
            채웁니다. 끌 수 없습니다.
          </div>
        </div>
      </div>

      <div className="pub-chips">
        <span className="pub-chip">
          <SearchIcon /> 주소류 (비움 고정) · {addressCount}
        </span>
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
            </div>

            {r.refs.map((ref) => (
              <div key={ref.path} className="pub-field">
                <span className="pub-field-ic addr"><SearchIcon /></span>
                <div className="pub-field-main">
                  <div className="pub-field-label">
                    {ref.label} <code>{ref.path}</code>
                  </div>
                  <div className="pub-field-val">
                    <span className="redacted">런타임 값</span>
                    <span className="arrow">→</span>
                    <span className="param">{ref.paramName}</span>
                  </div>
                </div>
                <span className="pub-blanked"><LockIcon /> 비워짐</span>
              </div>
            ))}

            {r.holes.map((h) =>
              h.kind === "address" ? (
                <div key={h.key} className="pub-field">
                  <span className="pub-field-ic addr"><SearchIcon /></span>
                  <div className="pub-field-main">
                    <div className="pub-field-label">
                      {h.label} <code>{h.path}</code>
                    </div>
                    <div className="pub-field-val">
                      <span className="redacted">{h.display}</span>
                      <span className="arrow">→</span>
                      <span className="param">{h.paramName}</span>
                    </div>
                  </div>
                  <span className="pub-blanked"><LockIcon /> 비워짐</span>
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
                      className={!keptNumbers.has(h.key) ? "on" : ""}
                      onClick={() => keptNumbers.has(h.key) && onToggleKeep(h.key)}
                    >
                      비우기
                      <small>{h.paramName}</small>
                    </button>
                    <button
                      type="button"
                      className={keptNumbers.has(h.key) ? "on" : ""}
                      onClick={() => !keptNumbers.has(h.key) && onToggleKeep(h.key)}
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
  addressCount: number;
  keptCount: number;
  numberCount: number;
}) {
  const {
    name,
    onName,
    description,
    onDescription,
    ruleCount,
    addressCount,
    keptCount,
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
          <b>{addressCount}</b>
        </div>
        <div className="pub-summary-row">
          <span>추천값 남김</span>
          <b>
            {keptCount} / {numberCount}
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
function stripPrefix(id: string): string {
  const PREFIX = "dashboard::";
  return id.startsWith(PREFIX) ? id.slice(PREFIX.length) : id;
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
