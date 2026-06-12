// Dambi onboarding(welcome) entry — 설치 직후 4-step 온보딩.
// store(DambiStore) 를 먼저 실행해야 welcome.js 가 window.DambiStore 를 읽는다.
import '../popup/dambi.css';
import './welcome.css';
import { deriveBaseline, derivePopupPackages } from '../popup/ps2-derive';

declare global {
  interface Window {
    DambiPs2: { derivePopupPackages: typeof derivePopupPackages; deriveBaseline: typeof deriveBaseline };
  }
}
window.DambiPs2 = { derivePopupPackages, deriveBaseline };

import '../popup/store.js';
import './welcome.js';
