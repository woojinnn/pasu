/**
 * /simulate — the 4-step simulation wizard (beta). Replaces the dense single
 * /simulation page with a guided flow: 지갑·상태 → 정책 → 트랜잭션 → 결과.
 *
 * Frontend-first: all data comes from {@link useSimController}'s MockProvider;
 * the backend (server state + sim-bridge WASM) is wired step-by-step later
 * without touching these views.
 */
import { Topbar } from "../../shell/Topbar";

import { StepNav } from "./StepNav";
import { Step1Wallets } from "./Step1Wallets";
import { Step2Policies } from "./Step2Policies";
import { Step3TxQueue } from "./Step3TxQueue";
import { Step4Results } from "./Step4Results";
import { useSimController } from "./useSimController";
import "./simulate.css";

export function SimulateWizardPage() {
  const c = useSimController();

  return (
    <div className="sw-page">
      <Topbar here="시뮬레이션" subtitle="베타 · 4단계" />

      <div className="sw-head">
        <StepNav step={c.step} goTo={c.goTo} />
      </div>

      <div className="sw-body">
        {c.step === 1 && <Step1Wallets c={c} />}
        {c.step === 2 && <Step2Policies c={c} />}
        {c.step === 3 && <Step3TxQueue c={c} />}
        {c.step === 4 && <Step4Results c={c} />}
      </div>

      <footer className="sw-foot">
        <button type="button" className="sw-btn ghost" disabled={c.step === 1} onClick={c.back}>
          ← 이전
        </button>
        <div className="sw-foot-spacer" />
        {c.step < 3 && (
          <button type="button" className="sw-btn primary" disabled={!c.canAdvance} onClick={c.next}>
            다음 →
          </button>
        )}
        {c.step === 3 && (
          <button
            type="button"
            className="sw-btn primary"
            disabled={!c.canAdvance || c.running}
            onClick={() => {
              c.run();
              c.next();
            }}
          >
            {c.running ? "실행 중…" : "시뮬레이션 실행 →"}
          </button>
        )}
        {c.step === 4 && (
          <button type="button" className="sw-btn primary" onClick={() => c.run()} disabled={c.running}>
            {c.running ? "실행 중…" : "다시 실행"}
          </button>
        )}
      </footer>
    </div>
  );
}
