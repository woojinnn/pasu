// Dambi popup entry — wires the Claude-designed popup UI (plain-JS) into the
// webpack page bundle. Order matters: `store.js` is an IIFE that publishes
// `window.DambiStore`, which `popup.js` reads at module top-level — so the
// store import MUST come first.
import Browser from 'webextension-polyfill';

import './dambi.css';
import './popup.css';
// `ps2-derive`: store.js가 패키지 카드를 파생할 때 쓰는 순수 TS 모듈 —
// store.js(IIFE) 평가 전에 전역(window.DambiPs2)으로 노출해야 한다.
import { deriveBaseline, derivePopupPackages } from './ps2-derive';

declare global {
  interface Window {
    DambiPs2: { derivePopupPackages: typeof derivePopupPackages; deriveBaseline: typeof deriveBaseline };
  }
}
window.DambiPs2 = { derivePopupPackages, deriveBaseline };

import './store.js';
import './popup.js';

// 팝업이 열렸다 = 알람 확인 — 마스코트 발바닥 배지를 초기화한다 (best-effort).
void Browser.runtime.sendMessage({ type: 'DAMBI_BADGE_SEEN' }).catch(() => {});
