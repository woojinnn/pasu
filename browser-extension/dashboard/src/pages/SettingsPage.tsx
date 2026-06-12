import { useState } from "react";
import { Trans, useTranslation } from "react-i18next";

/**
 * Server-environment toggle (reachable at `/settings`).
 *
 * Writes the runtime override to BOTH `localStorage` (dashboard) and
 * `chrome.storage.local` (service worker) under the same key, so one save
 * points the whole extension at a local/test server or prod without a
 * rebuild. The SW picks it up live (its `chrome.storage.onChanged`
 * listener); the dashboard reads it on next load — hence the reload hint.
 */
const KEY = "pasu_server_url";

// Labels are i18n keys, resolved at render time (never call t() at import time).
const PRESETS = [
  { labelKey: "settings.presetLocal", url: "http://127.0.0.1:8788" },
  { labelKey: "settings.presetProd", url: "https://pasu-policy.duckdns.org" },
];

const LANGUAGES: Array<{ code: "ko" | "en"; labelKey: string }> = [
  { code: "ko", labelKey: "settings.languageKo" },
  { code: "en", labelKey: "settings.languageEn" },
];

type ChromeStorageLocal = {
  set(items: Record<string, unknown>): Promise<void>;
  remove(key: string): Promise<void>;
};

/** Service-worker storage, only present when running as an extension page. */
function swStorage(): ChromeStorageLocal | undefined {
  return (globalThis as { chrome?: { storage?: { local?: ChromeStorageLocal } } }).chrome
    ?.storage?.local;
}

export function SettingsPage() {
  const { t, i18n } = useTranslation("common");
  const [value, setValue] = useState(() => window.localStorage.getItem(KEY) ?? "");
  const [saved, setSaved] = useState(false);

  const save = () => {
    const url = value.trim();
    if (url) window.localStorage.setItem(KEY, url);
    else window.localStorage.removeItem(KEY);

    const sw = swStorage();
    if (sw) {
      if (url) void sw.set({ [KEY]: url });
      else void sw.remove(KEY);
    }
    setSaved(true);
  };

  return (
    <div style={wrap}>
      <h1 style={{ fontSize: 20, marginBottom: 6 }}>{t("settings.title")}</h1>
      <p style={{ opacity: 0.7, fontSize: 13, marginTop: 0 }}>
        <Trans i18nKey="settings.desc" ns="common" components={{ code: <code /> }} />
      </p>

      <div style={{ display: "flex", gap: 8, margin: "14px 0" }}>
        {PRESETS.map((p) => (
          <button
            key={p.url}
            type="button"
            onClick={() => {
              setValue(p.url);
              setSaved(false);
            }}
            style={preset(value === p.url)}
          >
            <div style={{ fontWeight: 600 }}>{t(p.labelKey)}</div>
            <code style={{ fontSize: 11, opacity: 0.8 }}>{p.url}</code>
          </button>
        ))}
      </div>

      <input
        type="text"
        value={value}
        placeholder={t("settings.urlPlaceholder")}
        onChange={(e) => {
          setValue(e.target.value);
          setSaved(false);
        }}
        style={input}
      />

      <div style={{ marginTop: 14, display: "flex", alignItems: "center", gap: 12 }}>
        <button type="button" onClick={save} style={saveBtn}>
          {t("save")}
        </button>
        {saved && (
          <span style={{ fontSize: 13, opacity: 0.85 }}>
            <Trans
              i18nKey="settings.savedNote"
              ns="common"
              components={{ reload: <a onClick={() => window.location.reload()} style={link} /> }}
            />
          </span>
        )}
      </div>

      <h1 style={{ fontSize: 20, marginTop: 36, marginBottom: 6 }}>{t("settings.languageTitle")}</h1>
      <p style={{ opacity: 0.7, fontSize: 13, marginTop: 0 }}>{t("settings.languageDesc")}</p>
      <div style={{ display: "flex", gap: 8, margin: "14px 0" }}>
        {LANGUAGES.map((l) => (
          <button
            key={l.code}
            type="button"
            onClick={() => void i18n.changeLanguage(l.code)}
            style={preset(i18n.language === l.code)}
          >
            <div style={{ fontWeight: 600 }}>{t(l.labelKey)}</div>
            <code style={{ fontSize: 11, opacity: 0.8 }}>{l.code}</code>
          </button>
        ))}
      </div>
    </div>
  );
}

const wrap: React.CSSProperties = {
  maxWidth: 560,
  margin: "48px auto",
  padding: "0 20px",
  fontFamily: "system-ui, sans-serif",
};
const input: React.CSSProperties = {
  width: "100%",
  padding: "10px 12px",
  borderRadius: 8,
  border: "1px solid #333",
  background: "#161616",
  color: "inherit",
  boxSizing: "border-box",
};
const saveBtn: React.CSSProperties = {
  padding: "9px 18px",
  borderRadius: 8,
  border: "none",
  background: "#6ea8fe",
  color: "#06101f",
  fontWeight: 600,
  cursor: "pointer",
};
const link: React.CSSProperties = {
  color: "#6ea8fe",
  cursor: "pointer",
  textDecoration: "underline",
};
function preset(active: boolean): React.CSSProperties {
  return {
    flex: 1,
    padding: "10px 12px",
    borderRadius: 8,
    border: active ? "1px solid #6ea8fe" : "1px solid #333",
    background: active ? "#1b2740" : "#161616",
    color: "inherit",
    cursor: "pointer",
    textAlign: "left",
  };
}
