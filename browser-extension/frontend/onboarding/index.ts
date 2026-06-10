// Pasu onboarding(welcome) entry — 설치 직후 4-step 온보딩.
// store(PasuStore) 를 먼저 실행해야 welcome.js 가 window.PasuStore 를 읽는다.
import '../popup/pasu.css';
import './welcome.css';
import { deriveBaseline, derivePopupPackages } from '../popup/ps2-derive';

declare global {
  interface Window {
    PasuPs2: { derivePopupPackages: typeof derivePopupPackages; deriveBaseline: typeof deriveBaseline };
  }
}
window.PasuPs2 = { derivePopupPackages, deriveBaseline };

import '../popup/store.js';
import './welcome.js';
