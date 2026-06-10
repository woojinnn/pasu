/**
 * Generates a self-contained `field-explorer.html` — a wide-screen reference
 * that mirrors the form editor's pickers: choose 분류 → 동작 and see EVERY
 * condition field that action can offer (advanced fields included) with its
 * Korean label, type, unit, category, and description.
 *
 * It reuses the REAL form logic (`ACTION_GROUPS`, `fieldsForTrigger`) so the
 * page can never drift from what the editor actually shows. Bundle + run via
 * esbuild (see the npm `gen:explorer` shim / the one-liner in the PR notes).
 */
import { writeFileSync } from "fs";
import { join } from "path";

import { ACTION_GROUPS } from "../src/cedar/form/actions";
import { fieldsForTrigger, type FieldOption } from "../src/cedar/form/field-catalog";
import { ROLE_COLOUR, ROLE_LABEL_KO, type FieldKind, type Role } from "../src/editor-v9/gloss/paths";

// Output path: first CLI arg, else <cwd>/field-explorer.html.
const OUT = process.argv[2] ?? join(process.cwd(), "field-explorer.html");

/** Same TYPE vocabulary the FieldCombobox chip uses. */
function typeChip(fieldKind: FieldKind, role: Role): string {
  switch (fieldKind) {
    case "primitive.Bool": return "참/거짓";
    case "primitive.Long": return "숫자";
    case "primitive.decimal": return "소수";
    case "primitive.String": return role === "address" ? "주소" : "문자";
    case "ref": return "참조";
    case "collection": return "목록";
    case "record": return "레코드";
  }
}

interface FieldRow {
  label: string;
  path: string;
  type: string;
  role: Role;
  roleKo: string;
  unit: string;
  desc: string;
  advanced: boolean;
  source: "base" | "custom";
}

function rowOf(f: FieldOption): FieldRow {
  return {
    label: f.label,
    path: f.path,
    type: typeChip(f.fieldKind, f.role),
    role: f.role,
    roleKo: ROLE_LABEL_KO[f.role],
    unit: f.unit ?? "",
    desc: f.desc ?? "",
    advanced: Boolean(f.advanced),
    source: f.source,
  };
}

// Build the data: 분류 → 동작 → 조건 필드들.
const data = ACTION_GROUPS.map((g) => ({
  group: g.group,
  actions: g.actions.map((a) => {
    const fields = fieldsForTrigger({
      kind: "actionEq",
      entityType: a.entityType,
      id: a.id,
    }).map(rowOf);
    // prominent first, then advanced; stable label sort within each.
    fields.sort(
      (x, y) =>
        Number(x.advanced) - Number(y.advanced) ||
        x.role.localeCompare(y.role) ||
        x.label.localeCompare(y.label, "ko"),
    );
    return {
      id: a.id,
      label: a.label,
      entityType: a.entityType,
      fields,
      total: fields.length,
      advancedCount: fields.filter((f) => f.advanced).length,
    };
  }),
}));

const roleColors: Record<string, string> = {};
for (const r of Object.keys(ROLE_LABEL_KO) as Role[]) roleColors[r] = `hsl(${ROLE_COLOUR[r]} 55% 48%)`;

const totalActions = data.reduce((n, g) => n + g.actions.length, 0);

const html = `<!doctype html>
<html lang="ko">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>조건 필드 탐색기 — 분류 · 동작 · 필드</title>
<style>
  :root {
    --bg:#f6f7f9; --surface:#fff; --line:#e5e7eb; --line2:#eef0f3;
    --ink:#111827; --mut:#6b7280; --mut2:#9ca3af; --sel:#eef2ff; --selln:#c7d2fe;
    --adv:#fff7ed; --advln:#fed7aa; --chip:#f1f5f9; --chipink:#475569;
    --unit:#fef3c7; --unitink:#92400e; --cust:#f5f3ff; --custink:#7c3aed;
  }
  * { box-sizing:border-box; }
  body { margin:0; font:14px/1.5 -apple-system,BlinkMacSystemFont,"Segoe UI","Noto Sans KR",sans-serif; color:var(--ink); background:var(--bg); }
  header { padding:14px 20px; background:var(--surface); border-bottom:1px solid var(--line); display:flex; align-items:baseline; gap:14px; position:sticky; top:0; z-index:5; }
  header h1 { font-size:16px; margin:0; font-weight:700; }
  header .meta { color:var(--mut); font-size:12.5px; }
  header .right { margin-left:auto; display:flex; align-items:center; gap:14px; }
  header label { font-size:12.5px; color:var(--mut); display:flex; align-items:center; gap:6px; cursor:pointer; }
  header input[type=search] { font:13px inherit; padding:6px 10px; border:1px solid var(--line); border-radius:8px; width:220px; outline:none; }
  .wrap { display:grid; grid-template-columns:200px 260px 1fr; height:calc(100vh - 53px); }
  .col { overflow-y:auto; border-right:1px solid var(--line); background:var(--surface); }
  .col.fields { background:var(--bg); border-right:none; }
  .col h2 { font-size:11px; letter-spacing:.04em; text-transform:uppercase; color:var(--mut2); margin:0; padding:12px 16px 6px; position:sticky; top:0; background:var(--surface); }
  .item { padding:9px 16px; cursor:pointer; border-left:3px solid transparent; font-size:13.5px; }
  .item:hover { background:#f9fafb; }
  .item.sel { background:var(--sel); border-left-color:var(--selln); font-weight:600; }
  .item .sub { color:var(--mut2); font-size:11.5px; font-weight:400; }
  .item .cnt { float:right; color:var(--mut2); font-size:11px; font-variant-numeric:tabular-nums; }
  .fields h2 { background:transparent; }
  .fhead { padding:14px 20px 4px; }
  .fhead .t { font-size:18px; font-weight:700; }
  .fhead .s { color:var(--mut); font-size:12.5px; margin-top:2px; }
  .empty { padding:60px 20px; text-align:center; color:var(--mut2); }
  table { width:100%; border-collapse:collapse; }
  thead th { position:sticky; top:0; background:var(--bg); text-align:left; font-size:11px; color:var(--mut2); text-transform:uppercase; letter-spacing:.03em; padding:8px 12px; border-bottom:1px solid var(--line); }
  tbody tr { border-bottom:1px solid var(--line2); background:var(--surface); }
  tbody tr.adv { background:var(--adv); }
  tbody tr.hide { display:none; }
  td { padding:9px 12px; vertical-align:top; }
  td.label { font-weight:600; white-space:nowrap; }
  td.label .path { display:block; font:11px ui-monospace,Menlo,monospace; color:var(--mut2); font-weight:400; margin-top:1px; }
  .dot { display:inline-block; width:8px; height:8px; border-radius:50%; margin-right:6px; vertical-align:middle; }
  .chip { font-size:11px; border-radius:5px; padding:1px 7px; background:var(--chip); color:var(--chipink); white-space:nowrap; }
  .chip.unit { background:var(--unit); color:var(--unitink); }
  .badge { font-size:10.5px; border-radius:5px; padding:1px 6px; }
  .badge.adv { background:var(--advln); color:#9a3412; }
  .badge.cust { background:var(--cust); color:var(--custink); }
  td.desc { color:#374151; font-size:13px; max-width:560px; }
  td.cat { white-space:nowrap; color:var(--mut); font-size:12.5px; }
  .sechdr td { background:#fafbfc; font-size:11px; color:var(--mut2); text-transform:uppercase; letter-spacing:.04em; font-weight:700; padding:7px 12px; }
</style>
</head>
<body>
<header>
  <h1>조건 필드 탐색기</h1>
  <span class="meta">${data.length}개 분류 · ${totalActions}개 동작 — 폼 에디터의 “무엇을 감시하나요 → 언제 위험한가요” 그대로</span>
  <span class="right">
    <label><input type="checkbox" id="adv" checked /> 고급 필드 포함</label>
    <input type="search" id="q" placeholder="필드 검색 (라벨/경로/설명)…" />
  </span>
</header>
<div class="wrap">
  <div class="col" id="groups"><h2>분류</h2></div>
  <div class="col" id="actions"><h2>동작</h2></div>
  <div class="col fields" id="fields">
    <div class="empty">왼쪽에서 분류와 동작을 선택하세요.</div>
  </div>
</div>
<script>
const DATA = ${JSON.stringify(data)};
const ROLECLR = ${JSON.stringify(roleColors)};
let gi = 0, ai = -1;

const elG = document.getElementById('groups');
const elA = document.getElementById('actions');
const elF = document.getElementById('fields');
const elAdv = document.getElementById('adv');
const elQ = document.getElementById('q');

function renderGroups() {
  elG.innerHTML = '<h2>분류</h2>' + DATA.map((g,i) =>
    '<div class="item '+(i===gi?'sel':'')+'" data-g="'+i+'">'+g.group+
    '<span class="cnt">'+g.actions.length+'</span></div>').join('');
}
function renderActions() {
  const g = DATA[gi];
  elA.innerHTML = '<h2>동작</h2>' + g.actions.map((a,i) =>
    '<div class="item '+(i===ai?'sel':'')+'" data-a="'+i+'">'+a.label+
    '<span class="cnt">'+a.total+'</span>'+
    '<div class="sub">'+a.entityType+'::"'+a.id+'"</div></div>').join('');
}
function esc(s){ return (s||'').replace(/[&<>]/g, c=>({'&':'&amp;','<':'&lt;','>':'&gt;'}[c])); }

function renderFields() {
  if (ai < 0) { elF.innerHTML = '<div class="empty">동작을 선택하세요.</div>'; return; }
  const a = DATA[gi].actions[ai];
  const showAdv = elAdv.checked;
  const q = elQ.value.trim().toLowerCase();
  const match = f => !q || f.label.toLowerCase().includes(q) || f.path.toLowerCase().includes(q) || (f.desc||'').toLowerCase().includes(q);
  const rows = a.fields.filter(f => (showAdv || !f.advanced) && match(f));
  const prom = rows.filter(f=>!f.advanced), adv = rows.filter(f=>f.advanced);

  const rowHtml = f =>
    '<tr class="'+(f.advanced?'adv':'')+'">'+
      '<td class="label">'+esc(f.label)+
        (f.source==='custom'?' <span class="badge cust">보강</span>':'')+
        '<span class="path">'+esc(f.path)+'</span></td>'+
      '<td><span class="chip">'+esc(f.type)+'</span></td>'+
      '<td>'+(f.unit?'<span class="chip unit">'+esc(f.unit)+'</span>':'')+'</td>'+
      '<td class="cat"><span class="dot" style="background:'+(ROLECLR[f.role]||'#999')+'"></span>'+esc(f.roleKo)+'</td>'+
      '<td class="desc">'+esc(f.desc)+'</td>'+
    '</tr>';

  const sec = (title,list) => list.length? '<tr class="sechdr"><td colspan="5">'+title+' · '+list.length+'개</td></tr>'+list.map(rowHtml).join('') : '';

  elF.innerHTML =
    '<div class="fhead"><div class="t">'+esc(a.label)+'</div>'+
    '<div class="s">'+a.entityType+'::"'+a.id+'" — 조건 필드 '+a.total+'개 (전면 '+(a.total-a.advancedCount)+' · 고급 '+a.advancedCount+')</div></div>'+
    (rows.length===0 ? '<div class="empty">표시할 필드가 없습니다.</div>' :
      '<table><thead><tr><th>필드</th><th>종류</th><th>단위</th><th>카테고리</th><th>설명</th></tr></thead><tbody>'+
      sec('전면 필드', prom) + sec('고급 필드', adv) +
      '</tbody></table>');
}

elG.onclick = e => { const d=e.target.closest('[data-g]'); if(!d)return; gi=+d.dataset.g; ai=-1; renderGroups(); renderActions(); renderFields(); };
elA.onclick = e => { const d=e.target.closest('[data-a]'); if(!d)return; ai=+d.dataset.a; renderActions(); renderFields(); };
elAdv.onchange = renderFields;
elQ.oninput = renderFields;

renderGroups(); renderActions(); renderFields();
</script>
</body>
</html>`;

writeFileSync(OUT, html);
console.log(`wrote ${OUT}`);
console.log(`  ${data.length} groups, ${totalActions} actions`);
