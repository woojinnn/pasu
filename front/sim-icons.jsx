/* sim-icons.jsx — shared icon set (Lucide-style, 24-grid, currentColor).
   Exposes window.Icon({name, size}). */
const ICON_PATHS = {
  swap:     '<path d="M16 3l4 4-4 4"/><path d="M20 7H8a4 4 0 0 0-4 4"/><path d="M8 21l-4-4 4-4"/><path d="M4 17h12a4 4 0 0 0 4-4"/>',
  transfer: '<path d="M5 12h14"/><path d="M13 6l6 6-6 6"/>',
  stake:    '<path d="M12 2 3 7v10l9 5 9-5V7z"/><path d="M12 22V12"/><path d="m3 7 9 5 9-5"/>',
  restake:  '<path d="M21 12a9 9 0 1 1-3-6.7"/><path d="M21 3v5h-5"/>',
  check:    '<path d="M20 6 9 17l-5-5"/>',
  x:        '<path d="M18 6 6 18"/><path d="M6 6l12 12"/>',
  warn:     '<path d="M10.3 3.9 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0z"/><path d="M12 9v4"/><path d="M12 17h.01"/>',
  alert:    '<circle cx="12" cy="12" r="10"/><path d="M12 8v4"/><path d="M12 16h.01"/>',
  search:   '<circle cx="11" cy="11" r="7"/><path d="m21 21-4.3-4.3"/>',
  pin:      '<path d="M12 17v5"/><path d="M9 10.8V4h6v6.8l2 3.2H7z"/>',
  close:    '<path d="M18 6 6 18"/><path d="M6 6l12 12"/>',
  eye:      '<path d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7-10-7-10-7z"/><circle cx="12" cy="12" r="3"/>',
  layers:   '<path d="m12 2 9 5-9 5-9-5 9-5z"/><path d="m3 12 9 5 9-5"/><path d="m3 17 9 5 9-5"/>',
  single:   '<rect x="3" y="3" width="18" height="18" rx="2"/>',
  plus:     '<path d="M12 5v14"/><path d="M5 12h14"/>',
  minus:    '<path d="M5 12h14"/>',
  code:     '<path d="m16 18 6-6-6-6"/><path d="m8 6-6 6 6 6"/>',
  form:     '<rect x="3" y="3" width="18" height="18" rx="2"/><path d="M7 8h10"/><path d="M7 12h6"/><path d="M7 16h8"/>',
  library:  '<path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20"/><path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z"/>',
  wallet:   '<path d="M19 7V5a2 2 0 0 0-2-2H5a2 2 0 0 0 0 4h15a1 1 0 0 1 1 1v4a1 1 0 0 1-1 1H5a2 2 0 0 1-2-2V5"/><path d="M17 12h.01"/>',
  gavel:    '<path d="m14 13-7.5 7.5a2.1 2.1 0 0 1-3-3L11 10"/><path d="m16 16 6-6"/><path d="m8 8 6-6"/><path d="m9 7 8 8"/><path d="m21 11-8-8"/>',
  shield:   '<path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/>',
  zap:      '<path d="M13 2 3 14h9l-1 8 10-12h-9z"/>',
  play:     '<path d="M6 4l14 8-14 8z"/>',
  pause:    '<path d="M7 4h3v16H7z"/><path d="M14 4h3v16h-3z"/>',
  info:     '<circle cx="12" cy="12" r="10"/><path d="M12 16v-4"/><path d="M12 8h.01"/>',
  chevron:  '<path d="m6 9 6 6 6-6"/>',
  lock:     '<rect x="3" y="11" width="18" height="11" rx="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/>',
  conflict: '<path d="M8.5 8.5 3 3"/><path d="m21 21-5.5-5.5"/><path d="M3 21 21 3"/>',
  bolt:     '<path d="M12 2v4"/><path d="m4.9 4.9 2.9 2.9"/><path d="M2 12h4"/><path d="M12 18v4"/><circle cx="12" cy="12" r="4"/>',
  reset:    '<path d="M3 12a9 9 0 1 0 9-9 9 9 0 0 0-6.7 3"/><path d="M3 3v5h5"/>',
  dot:      '<circle cx="12" cy="12" r="4"/>',
  target:   '<circle cx="12" cy="12" r="9"/><circle cx="12" cy="12" r="4"/>',
  supply:   '<path d="M12 3v10"/><path d="m8 9 4 4 4-4"/><path d="M4 21h16"/><path d="M4 17h16"/>',
  borrow:   '<path d="M12 21V11"/><path d="m8 15 4-4 4 4"/><path d="M4 7h16"/><path d="M4 3h16"/>',
  bank:     '<path d="M3 21h18"/><path d="M3 10h18"/><path d="m12 3 9 5H3z"/><path d="M5 10v11"/><path d="M19 10v11"/><path d="M12 10v11"/>',
  home:     '<path d="m3 10 9-7 9 7"/><path d="M5 9v11h14V9"/>',
  monitor:  '<path d="M3 4h18v12H3z"/><path d="M8 20h8"/><path d="M12 16v4"/><path d="m7 11 3-3 2 2 4-4"/>',
  audit:    '<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><path d="M14 2v6h6"/><path d="m9 15 2 2 4-4"/>',
  settings: '<circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/>',
  blocks:   '<rect x="3" y="3" width="7" height="7" rx="1"/><rect x="14" y="3" width="7" height="7" rx="1"/><rect x="3" y="14" width="7" height="7" rx="1"/><rect x="14" y="14" width="7" height="7" rx="1"/>',
  link:     '<path d="M9 17H7A5 5 0 0 1 7 7h2"/><path d="M15 7h2a5 5 0 0 1 0 10h-2"/><path d="M8 12h8"/>',
  gauge:    '<path d="M12 14a2 2 0 1 0 0-4 2 2 0 0 0 0 4z"/><path d="M13.4 10.6 19 5"/><path d="M3 12a9 9 0 0 1 18 0"/>',
  ghost:    '<path d="M9 10h.01"/><path d="M15 10h.01"/><path d="M12 2a8 8 0 0 0-8 8v12l3-2 2 2 3-2 3 2 2-2 3 2V10a8 8 0 0 0-8-8z"/>',
  pencil:   '<path d="M12 20h9"/><path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4z"/>',
  grip:     '<circle cx="9" cy="6" r="1"/><circle cx="15" cy="6" r="1"/><circle cx="9" cy="12" r="1"/><circle cx="15" cy="12" r="1"/><circle cx="9" cy="18" r="1"/><circle cx="15" cy="18" r="1"/>',
  expand:   '<path d="M15 3h6v6"/><path d="M9 21H3v-6"/><path d="M21 3l-7 7"/><path d="M3 21l7-7"/>',
  collapse: '<path d="M4 14h6v6"/><path d="M20 10h-6V4"/><path d="M14 10l7-7"/><path d="M3 21l7-7"/>',
};
function Icon({ name, size = 18, style }) {
  const p = ICON_PATHS[name] || "";
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor"
      strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" style={style}
      dangerouslySetInnerHTML={{ __html: p }} />
  );
}
window.Icon = Icon;
window.CAT_ICON = { amm: "swap", lending: "bank", token: "transfer", airdrop: "library", perp: "gauge", launchpad: "zap", multicall: "blocks" };
window.ACT_ICON = { swap: "swap", supply: "supply", borrow: "borrow", transfer: "transfer" };
