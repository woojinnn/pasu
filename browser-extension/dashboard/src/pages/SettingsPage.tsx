import { useState } from "react";

/**
 * Server-environment toggle (reachable at `/settings`).
 *
 * Writes the runtime override to BOTH `localStorage` (dashboard) and
 * `chrome.storage.local` (service worker) under the same key, so one save
 * points the whole extension at a local/test server or prod without a
 * rebuild. The SW picks it up live (its `chrome.storage.onChanged`
 * listener); the dashboard reads it on next load — hence the reload hint.
 */
const KEY = "scopeball_server_url";

const PRESETS = [
  { label: "로컬 (테스트)", url: "http://127.0.0.1:8788" },
  { label: "프로덕션", url: "https://pasu-policy.duckdns.org" },
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
      <h1 style={{ fontSize: 20, marginBottom: 6 }}>설정 — 서버 환경</h1>
      <p style={{ opacity: 0.7, fontSize: 13, marginTop: 0 }}>
        대시보드 + 서비스워커가 호출할 policy-server 주소. 비우면 빌드 기본값
        (<code>SCOPEBALL_SERVER_URL</code>)을 사용합니다.
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
            <div style={{ fontWeight: 600 }}>{p.label}</div>
            <code style={{ fontSize: 11, opacity: 0.8 }}>{p.url}</code>
          </button>
        ))}
      </div>

      <input
        type="text"
        value={value}
        placeholder="http://127.0.0.1:8788  (비우면 빌드 기본값)"
        onChange={(e) => {
          setValue(e.target.value);
          setSaved(false);
        }}
        style={input}
      />

      <div style={{ marginTop: 14, display: "flex", alignItems: "center", gap: 12 }}>
        <button type="button" onClick={save} style={saveBtn}>
          저장
        </button>
        {saved && (
          <span style={{ fontSize: 13, opacity: 0.85 }}>
            저장됨 — 서비스워커 즉시 적용. 대시보드는{" "}
            <a onClick={() => window.location.reload()} style={link}>
              새로고침
            </a>{" "}
            후 적용.
          </span>
        )}
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
