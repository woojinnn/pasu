// Pasu popup entry — wires the Claude-designed popup UI (plain-JS) into the
// webpack page bundle. Order matters: `store.js` is an IIFE that publishes
// `window.PasuStore`, which `popup.js` reads at module top-level — so the
// store import MUST come first.
import './pasu.css';
import './popup.css';
// `ps2-derive`: store.js가 패키지 카드를 파생할 때 쓰는 순수 TS 모듈 —
// store.js(IIFE) 평가 전에 전역(window.PasuPs2)으로 노출해야 한다.
import { deriveBaseline, derivePopupPackages } from './ps2-derive';

declare global {
  interface Window {
    PasuPs2: { derivePopupPackages: typeof derivePopupPackages; deriveBaseline: typeof deriveBaseline };
  }
}
window.PasuPs2 = { derivePopupPackages, deriveBaseline };

import './store.js';
import './popup.js';
