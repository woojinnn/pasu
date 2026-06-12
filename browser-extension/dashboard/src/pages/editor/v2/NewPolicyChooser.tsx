import { useNavigate } from "react-router-dom";
import { useTranslation } from "react-i18next";
import type { TFunction } from "i18next";

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
  preview: "form" | "cedar";
  disabled?: boolean;
  disabledNote?: string;
}

/** 카드 문구는 호출 시점에 t()로 — 모듈 평가 시점엔 i18n이 없을 수 있다. */
function buildCards(t: TFunction): CardDef[] {
  return [
    {
      key: "form",
      accent: "cyan",
      title: t("editor:chooser.form.title"),
      summary: t("editor:chooser.form.summary"),
      rec: t("editor:chooser.form.rec"),
      pros: [
        t("editor:chooser.form.pro1"),
        t("editor:chooser.form.pro2"),
        t("editor:chooser.form.pro3"),
      ],
      cons: [t("editor:chooser.form.con1")],
      preview: "form",
    },
    {
      key: "cedar",
      accent: "slate",
      title: t("editor:chooser.cedar.title"),
      summary: t("editor:chooser.cedar.summary"),
      rec: t("editor:chooser.cedar.rec"),
      pros: [t("editor:chooser.cedar.pro1"), t("editor:chooser.cedar.pro2")],
      cons: [t("editor:chooser.cedar.con1"), t("editor:chooser.cedar.con2")],
      preview: "cedar",
    },
  ];
}

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
  const { t } = useTranslation("editor");

  if (!open) return null;

  const cards = buildCards(t);

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
        newPolicy: { method, cedarText: seedCedar(slug), displayName: t("chooser.newPolicyName") },
      },
    });
  };

  return (
    <div className="ev2-modal-bd" role="dialog" aria-modal onClick={onClose}>
      <div className="ev2-mpc" onClick={(e) => e.stopPropagation()}>
        <div className="ev2-mpc-h">
          <div>
            <div className="t">{t("chooser.title")}</div>
            <div className="s">{t("chooser.subtitle")}</div>
          </div>
          <button
            type="button"
            className="ev2-mpc-x"
            onClick={onClose}
            aria-label={t("common:close")}
          >
            <XIcon />
          </button>
        </div>
        <div className="ev2-mpc-grid">
          {cards.map((c) => {
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
                    <span className="ev2-mpc-soon">{t("chooser.soon")}</span>
                  )}
                </div>
                <ChooserPreview kind={c.preview} />
                <div className="ev2-mpc-summary">{c.summary}</div>
                <div className="ev2-mpc-rec">
                  <span className="lbl">{t("chooser.recLabel")}</span>
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
                  {c.disabled ? c.disabledNote : t("chooser.start")}
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

function ChooserPreview({ kind }: { kind: "form" | "cedar" }) {
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
