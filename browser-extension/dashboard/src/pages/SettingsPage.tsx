import { useEffect, useState } from "react";
import {
  DEFAULT_PREFERENCES,
  loadPreferences,
  savePreferences,
  type Preferences,
} from "../settings/preferences";
import "./SettingsPage.css";

export function SettingsPage() {
  const [prefs, setPrefs] = useState<Preferences>(() => loadPreferences());
  const [savedAt, setSavedAt] = useState<number | null>(null);

  useEffect(() => {
    savePreferences(prefs);
    setSavedAt(Date.now());
  }, [prefs]);

  const handle = <K extends keyof Preferences>(
    key: K,
    value: Preferences[K],
  ) => {
    setPrefs((p) => ({ ...p, [key]: value }));
  };

  const reset = () => setPrefs(DEFAULT_PREFERENCES);

  return (
    <div className="settings-page">
      <header className="settings-header">
        <h1>Settings</h1>
        {savedAt ? <span className="settings-saved">자동 저장됨</span> : null}
      </header>

      <section className="settings-section card">
        <header className="section-head">
          <h2>Policy Test 기본값</h2>
          <p>Editor의 Policy Test 패널 폼이 이 값으로 시작합니다.</p>
        </header>
        <div className="settings-grid">
          <Field
            label="Chain ID"
            hint="EVM chain id (예: 1 = Mainnet, 137 = Polygon)"
          >
            <input
              type="number"
              min={1}
              value={prefs.policyTestChainId}
              onChange={(e) => {
                const n = Number.parseInt(e.target.value, 10);
                if (Number.isFinite(n) && n > 0) {
                  handle("policyTestChainId", n);
                }
              }}
            />
          </Field>
          <Field label="기본 From (actor) 주소">
            <input
              type="text"
              value={prefs.policyTestActor}
              onChange={(e) => handle("policyTestActor", e.target.value)}
              spellCheck={false}
            />
          </Field>
          <Field label="기본 To 주소">
            <input
              type="text"
              value={prefs.policyTestTo}
              onChange={(e) => handle("policyTestTo", e.target.value)}
              spellCheck={false}
            />
          </Field>
        </div>
      </section>

      <section className="settings-section card">
        <header className="section-head">
          <h2>동기화</h2>
          <p>
            확장 측 카탈로그가 변경됐을 때 Dashboard를 어떻게 처리할지 결정합니다.
          </p>
        </header>
        <Toggle
          label="자동 새로고침"
          hint="extension SW에서 정책 카탈로그가 바뀌면 자동으로 다시 불러옵니다."
          checked={prefs.autoRefreshOnChange}
          onChange={(v) => handle("autoRefreshOnChange", v)}
        />
      </section>

      <section className="settings-section card">
        <header className="section-head">
          <h2>초기화</h2>
          <p>모든 설정을 기본값으로 되돌립니다 (localStorage만 영향).</p>
        </header>
        <button type="button" className="settings-reset" onClick={reset}>
          기본값으로 되돌리기
        </button>
      </section>
    </div>
  );
}

function Field({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    <label className="settings-field">
      <span className="settings-field-label">{label}</span>
      {children}
      {hint ? <span className="settings-field-hint">{hint}</span> : null}
    </label>
  );
}

function Toggle({
  label,
  hint,
  checked,
  onChange,
}: {
  label: string;
  hint: string;
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <label className="settings-toggle">
      <input
        type="checkbox"
        checked={checked}
        onChange={(e) => onChange(e.target.checked)}
      />
      <span className="settings-toggle-text">
        <span className="settings-toggle-label">{label}</span>
        <span className="settings-toggle-hint">{hint}</span>
      </span>
    </label>
  );
}
