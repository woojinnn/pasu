/**
 * Lightweight read-only syntax highlighter for the market detail page, so a
 * user can transparently inspect exactly what cedar / manifest they're about
 * to install. Tokenizes with sticky regexes (no deps) into colored spans.
 *
 * Not a full parser — display-only. Cedar + JSON are the only langs needed.
 */
import { useState } from "react";

type Tok = { cls: string | null; text: string };
type Rule = { re: RegExp; cls: string | ((m: string) => string | null) };

function scan(text: string, rules: Rule[]): Tok[] {
  const out: Tok[] = [];
  const push = (cls: string | null, t: string) => {
    const last = out[out.length - 1];
    if (last && last.cls === cls) last.text += t;
    else out.push({ cls, text: t });
  };
  let i = 0;
  while (i < text.length) {
    let hit: Tok | null = null;
    for (const r of rules) {
      r.re.lastIndex = i;
      const m = r.re.exec(text);
      if (m && m.index === i && m[0].length > 0) {
        const cls = typeof r.cls === "function" ? r.cls(m[0]) : r.cls;
        hit = { cls, text: m[0] };
        break;
      }
    }
    if (hit) {
      push(hit.cls, hit.text);
      i += hit.text.length;
    } else {
      push(null, text[i]);
      i += 1;
    }
  }
  return out;
}

const CEDAR_KW = new Set([
  "permit", "forbid", "when", "unless", "if", "then", "else", "in", "has", "like", "true", "false",
]);
const CEDAR_BUILTIN = new Set(["principal", "action", "resource", "context"]);

const CEDAR_RULES: Rule[] = [
  { re: /\/\/[^\n]*/y, cls: "cv-comment" },
  { re: /"(?:\\.|[^"\\])*"/y, cls: "cv-string" },
  { re: /@[A-Za-z_]\w*/y, cls: "cv-annot" },
  { re: /\d+(?:\.\d+)?/y, cls: "cv-num" },
  {
    re: /[A-Za-z_]\w*(?:::[A-Za-z_]\w*)*/y,
    cls: (m) => {
      if (CEDAR_KW.has(m)) return "cv-kw";
      if (CEDAR_BUILTIN.has(m.split("::")[0])) return "cv-builtin";
      if (m.includes("::")) return "cv-type";
      return null;
    },
  },
  { re: /==|!=|<=|>=|&&|\|\||[=<>!]/y, cls: "cv-op" },
];

const JSON_RULES: Rule[] = [
  { re: /"(?:\\.|[^"\\])*"(?=\s*:)/y, cls: "cv-key" },
  { re: /"(?:\\.|[^"\\])*"/y, cls: "cv-string" },
  { re: /(?:true|false|null)(?![\w])/y, cls: "cv-lit" },
  { re: /-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?/y, cls: "cv-num" },
];

export function CodeView({ code, lang }: { code: string; lang: "cedar" | "json" }) {
  const toks = scan(code, lang === "cedar" ? CEDAR_RULES : JSON_RULES);
  return (
    <pre className="cv">
      <code>
        {toks.map((t, i) =>
          t.cls ? (
            <span key={i} className={t.cls}>
              {t.text}
            </span>
          ) : (
            <span key={i}>{t.text}</span>
          ),
        )}
      </code>
    </pre>
  );
}

/**
 * Tabbed viewer: `policy.cedar` + `manifest.json`. Surfaces the exact bytes a
 * user installs. `manifest` is the parsed object (pretty-printed here).
 */
export function CodeTabs({
  cedar,
  manifest,
  locale,
  hideComments = false,
}: {
  cedar?: string | null;
  manifest?: unknown;
  locale: "ko" | "en";
  /** Strip `//` comments from the cedar tab (the long English rationale lives
   * in the Korean description above the code instead). */
  hideComments?: boolean;
}) {
  const shownCedar = cedar ? (hideComments ? stripCedarComments(cedar) : cedar) : null;
  const manifestStr =
    manifest != null ? JSON.stringify(manifest, null, 2) : null;
  const [tab, setTab] = useState<"cedar" | "manifest">(shownCedar ? "cedar" : "manifest");
  const [copied, setCopied] = useState(false);
  const active = tab === "cedar" ? shownCedar ?? "" : manifestStr ?? "";

  const copy = () => {
    try {
      void navigator.clipboard?.writeText(active);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1200);
    } catch {
      /* clipboard unavailable — no-op */
    }
  };

  return (
    <div className="codetabs">
      <div className="codetabs-bar">
        {shownCedar && (
          <button
            type="button"
            className={tab === "cedar" ? "is-active" : ""}
            onClick={() => setTab("cedar")}
          >
            policy.cedar
          </button>
        )}
        {manifestStr && (
          <button
            type="button"
            className={tab === "manifest" ? "is-active" : ""}
            onClick={() => setTab("manifest")}
          >
            manifest.json
          </button>
        )}
        <button type="button" className="codetabs-copy" onClick={copy}>
          {copied ? (locale === "ko" ? "복사됨" : "Copied") : locale === "ko" ? "복사" : "Copy"}
        </button>
      </div>
      {tab === "cedar" && shownCedar && <CodeView code={shownCedar} lang="cedar" />}
      {tab === "manifest" && manifestStr && <CodeView code={manifestStr} lang="json" />}
    </div>
  );
}

/** Remove `//` comments from cedar source (string-aware), collapsing the
 * blank lines they leave behind. Display-only — the installed body keeps them. */
export function stripCedarComments(src: string): string {
  const lines = src.split("\n").map((line) => {
    let inStr = false;
    let cut = -1;
    for (let i = 0; i < line.length; i++) {
      const c = line[i];
      if (c === '"' && line[i - 1] !== "\\") inStr = !inStr;
      else if (!inStr && c === "/" && line[i + 1] === "/") {
        cut = i;
        break;
      }
    }
    return cut >= 0 ? line.slice(0, cut).replace(/\s+$/, "") : line;
  });
  return lines.join("\n").replace(/\n{3,}/g, "\n\n").replace(/^\s*\n+/, "").trimEnd();
}

/** Pull the leading `//` comment block from a cedar policy as a provisional
 * human summary (until authored `docs` exist). Returns "" when none. */
export function leadingComment(cedar: string): string {
  const out: string[] = [];
  for (const raw of cedar.split("\n")) {
    const t = raw.trim();
    if (t.startsWith("//")) out.push(t.replace(/^\/\/+\s?/, ""));
    else if (t === "" && out.length === 0) continue;
    else break;
  }
  // Collapse to the first 2 sentences so the inline summary stays tight.
  const joined = out.join(" ").replace(/\s+/g, " ").trim();
  const sentences = joined.split(/(?<=[.!?。])\s+/).slice(0, 2).join(" ");
  return sentences || joined;
}
