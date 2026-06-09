import { useNavigate } from "react-router-dom";

import {
  dashboardId,
  type PolicyMethod,
} from "../../../server-api";

import { CaretRightIcon, CheckIcon, ShieldIcon, XIcon } from "./icons";

interface CardDef {
  key: PolicyMethod;
  accent: "cyan" | "sage" | "slate";
  title: string;
  summary: string;
  rec: string;
  pros: string[];
  cons: string[];
  preview: "form" | "block" | "cedar";
  disabled?: boolean;
  disabledNote?: string;
}

const CARDS: CardDef[] = [
  {
    key: "form",
    accent: "cyan",
    title: "폼으로 만들기",
    summary:
      "가장 쉬움 · 흔한 정책(forbid + AND) · .cedar와 manifest 자동 생성 · 임계값만 바꾸면 끝.",
    rec: "처음·표준 정책",
    pros: ["round-trip 안전망", "cedar·manifest 자동", "인라인 값 편집"],
    cons: ["복잡한 정책(OR·중첩 등)은 폼으로 안 열릴 수 있어요"],
    preview: "form",
  },
  {
    key: "block",
    accent: "sage",
    title: "블록으로 만들기",
    summary:
      "OR·has·중첩 등 복잡한 조건까지 시각적으로 조립 · 전체 Cedar 표현.",
    rec: "복잡한 로직",
    pros: ["OR · has · set", "시각적 조립", "전체 AST(만능)"],
    cons: ["폼보다 손이 감"],
    preview: "block",
  },
  {
    key: "cedar",
    accent: "slate",
    title: "Cedar로 만들기",
    summary:
      "코드 직접 작성 · 최대 자유, 가드 최소 · 폼 안전망 밖 · 숙련자용.",
    rec: "Cedar를 아는 사람",
    pros: ["최대 자유", "manifest 직접 관리"],
    cons: ["가드 최소", "폼 안전망 밖"],
    preview: "cedar",
  },
];

/**
 * Minimal seed cedar so the draft validates on save. Real authoring
 * happens in the editor view; this body is replaced as soon as the
 * user types anything.
 */
function seedCedar(id: string): string {
  return `// @id("${id}")\nforbid (\n  principal,\n  action,\n  resource\n);`;
}

interface ChooserProps {
  open: boolean;
  onClose: () => void;
}

export function NewPolicyChooser({ open, onClose }: ChooserProps) {
  const navigate = useNavigate();

  if (!open) return null;

  // Do NOT persist here. We hand the editor an in-memory seed via navigation
  // state; nothing is written to storage until the user presses 저장. So a
  // policy the user abandons without saving simply never exists.
  const pick = (method: PolicyMethod) => {
    const stamp = Date.now().toString(36);
    const slug = `new-${method}-${stamp}`;
    const id = dashboardId(slug);
    onClose();
    navigate(`/editor/${encodeURIComponent(id)}`, {
      state: {
        newPolicy: { method, cedarText: seedCedar(slug), displayName: "새 정책" },
      },
    });
  };

  return (
    <div className="ev2-modal-bd" role="dialog" aria-modal onClick={onClose}>
      <div className="ev2-mpc" onClick={(e) => e.stopPropagation()}>
        <div className="ev2-mpc-h">
          <div>
            <div className="t">새 정책 만들기</div>
            <div className="s">
              어떤 방식으로 시작할지 고르세요. 셋 다 같은 Cedar로 저장되고,
              나중에 다른 방식으로도 볼 수 있어요 (폼은 단순한 정책만).
            </div>
          </div>
          <button
            type="button"
            className="ev2-mpc-x"
            onClick={onClose}
            aria-label="닫기"
          >
            <XIcon />
          </button>
        </div>
        <div className="ev2-mpc-grid">
          {CARDS.map((c) => {
            const disabled = !!c.disabled;
            const cls = [
              "ev2-mpc-card",
              c.accent,
              c.disabled ? "is-disabled" : "",
            ]
              .filter(Boolean)
              .join(" ");
            return (
              <button
                key={c.key}
                type="button"
                className={cls}
                disabled={disabled}
                onClick={() => pick(c.key)}
                title={c.disabled ? c.disabledNote : undefined}
              >
                <div className="ev2-mpc-card-top">
                  <span className="ev2-mpc-ic">
                    <ShieldIcon />
                  </span>
                  <span className="ev2-mpc-title">{c.title}</span>
                  {c.disabled && (
                    <span className="ev2-mpc-soon">준비 중</span>
                  )}
                </div>
                <ChooserPreview kind={c.preview} />
                <div className="ev2-mpc-summary">{c.summary}</div>
                <div className="ev2-mpc-rec">
                  <span className="lbl">추천</span>
                  {c.rec}
                </div>
                <div className="ev2-mpc-pc">
                  <ul className="pros">
                    {c.pros.map((p, i) => (
                      <li key={i}>
                        <CheckIcon />
                        {p}
                      </li>
                    ))}
                  </ul>
                  <ul className="cons">
                    {c.cons.map((p, i) => (
                      <li key={i}>
                        <XIcon />
                        {p}
                      </li>
                    ))}
                  </ul>
                </div>
                <span className="ev2-mpc-go">
                  {c.disabled ? c.disabledNote : "이 방식으로 시작"}
                  {!c.disabled && <CaretRightIcon />}
                </span>
              </button>
            );
          })}
        </div>
      </div>
    </div>
  );
}

function ChooserPreview({ kind }: { kind: "form" | "block" | "cedar" }) {
  if (kind === "form") {
    return (
      <div className="ev2-mpc-prev form">
        <div className="row">
          <span className="cap" />
          <span className="fld" />
          <span className="op">&gt;</span>
          <span className="val">150</span>
        </div>
        <div className="and">AND</div>
        <div className="row">
          <span className="cap" />
          <span className="fld w2" />
          <span className="op">≠</span>
          <span className="val ref">self</span>
        </div>
      </div>
    );
  }
  if (kind === "block") {
    return (
      <div className="ev2-mpc-prev block">
        <div className="hat" />
        <div className="or">
          <span className="spine" />
          <div className="chip" />
          <div className="chip w2" />
        </div>
      </div>
    );
  }
  return (
    <div className="ev2-mpc-prev cedar">
      <div className="ln">
        <span className="g" />
        <span className="t kw" />
      </div>
      <div className="ln">
        <span className="g" />
        <span className="t" />
      </div>
      <div className="ln">
        <span className="g" />
        <span className="t guard" />
      </div>
      <div className="ln">
        <span className="g" />
        <span className="t s" />
      </div>
    </div>
  );
}
