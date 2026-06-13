// editor-v6-modals.jsx — custom schema (full form) + folder creation modals.

const { useState: useSMod } = React;

const V6_LEAF_TYPES = ['seconds', 'usd', 'bps', 'boolean', 'address', 'enum', 'number'];

// flatten custom folders for the "target location" dropdown
function v6Folders(tree, depth = 0, acc = []) {
  for (const n of tree || []) {
    if (n.kind === 'folder') {
      acc.push({ id: n.id, name: n.name, depth });
      v6Folders(n.children, depth + 1, acc);
    }
  }
  return acc;
}

function V6SchemaModal({ onClose, onSubmit, customTree, defaultParent }) {
  const [name, setName] = useSMod('');
  const [leafType, setLeafType] = useSMod('bps');
  const [shape, setShape] = useSMod('rounded');
  const [tone, setTone] = useSMod(null);
  const [absence, setAbsence] = useSMod('false');
  const [parentId, setParentId] = useSMod(defaultParent || '');
  const folders = v6Folders(customTree);
  const th = (tone && V6_TONE_HEX[tone]) ? V6_TONE_HEX[tone] : { fill: V6_NEUTRAL_FILL, border: V6_NEUTRAL_BORDER };

  const clean = name.trim().replace(/\.cedarschema$/i, '').replace(/\s+/g, '_');
  const valid = clean.length > 0;
  const fullName = clean ? `${clean}.cedarschema` : 'name.cedarschema';

  const submit = () => {
    if (!valid) return;
    onSubmit({
      file: { name: fullName, leafType, tone, shape, absence },
      parentId: parentId || null,
    });
  };

  return (
    <div className="scrim" onClick={onClose}>
      <div className="v6modal" onClick={(e) => e.stopPropagation()}>
        <div className="v6m-h">
          <div className="v6m-ic"><V6I.filePlus /></div>
          <div className="v6m-htxt">
            <div className="v6m-eye">Custom · enrichment</div>
            <div className="v6m-t">새 manifest 블록 정의</div>
            <div className="v6m-d">manifest = 커스텀 블록 스키마 단위. 정의하면 커스텀 트리에 추가되고 즉시 드래그할 수 있습니다.</div>
          </div>
          <button className="v6m-x" onClick={onClose}><V5I.x style={{ width: 14, height: 14 }} /></button>
        </div>

        <div className="v6m-body">
          <div className="v6m-field">
            <span className="v6m-field-l">파일명 <span className="req">*</span></span>
            <div className="v6m-input-wrap">
              <input className="v6m-input mono" autoFocus value={name}
                placeholder="effectiveRateVsOracleBps"
                onChange={(e) => setName(e.target.value)}
                onKeyDown={(e) => e.key === 'Enter' && submit()} />
              <span className="suffix">.cedarschema</span>
            </div>
          </div>

          <div className="v6m-2col">
            <div className="v6m-field">
              <span className="v6m-field-l">타입</span>
              <div className="v6m-seg">
                {V6_LEAF_TYPES.map(t => (
                  <button key={t} className={leafType === t ? 'on' : ''} onClick={() => setLeafType(t)}>{t}</button>
                ))}
              </div>
            </div>
            <div className="v6m-field">
              <span className="v6m-field-l">값 부재 시</span>
              <div className="v6m-seg">
                <button className={absence === 'false' ? 'on' : ''} onClick={() => setAbsence('false')}>false</button>
                <button className={absence === 'true' ? 'on' : ''} onClick={() => setAbsence('true')}>true</button>
              </div>
              <span className="hint" style={{ fontFamily: 'var(--ff-mono)', fontSize: 10, color: 'var(--slate-400)' }}>
                enrichment 값이 없을 때 {absence === 'true' ? '조건을 참으로 간주' : '조건을 거짓으로 간주'}
              </span>
            </div>
          </div>

          <div className="v6m-2col">
            <div className="v6m-field">
              <span className="v6m-field-l">모양</span>
              <div className="v6m-shapes">
                {V6_SHAPES.map(sh => (
                  <button key={sh} className={`v6m-shape ${shape === sh ? 'on' : ''}`} onClick={() => setShape(sh)}>
                    <span className={`vsw ${sh}`} />
                    <span className="vlab">{V6_SHAPE_LABEL[sh]}</span>
                  </button>
                ))}
              </div>
            </div>
            <div className="v6m-field">
              <span className="v6m-field-l">심각도 · status <span className="hint">엣지+pill (색 아님)</span></span>
              <div className="v6m-tones">
                <button className={`v6m-tone ${tone === null ? 'on' : ''}`} onClick={() => setTone(null)}>
                  <span className="vts" style={{ background: 'var(--surface)', borderColor: 'var(--hairline)' }} />
                  <span className="vtl">없음</span>
                </button>
                {['fail', 'warn', 'pass'].map(t => (
                  <button key={t} className={`v6m-tone ${tone === t ? 'on' : ''}`} onClick={() => setTone(t)}>
                    <span className="vts" style={{ background: V6_SEV[t].edge, borderColor: V6_SEV[t].edge }} />
                    <span className="vtl">{V6_TONES[t].label}</span>
                  </button>
                ))}
              </div>
            </div>
          </div>

          <div className="v6m-field">
            <span className="v6m-field-l">대상 폴더</span>
            <select className="v6m-input" value={parentId} onChange={(e) => setParentId(e.target.value)}>
              <option value="">커스텀 루트</option>
              {folders.map(f => (
                <option key={f.id} value={f.id}>{'\u00A0'.repeat(f.depth * 2)}{f.depth > 0 ? '└ ' : ''}{f.name}/</option>
              ))}
            </select>
          </div>

          <div className="v6m-preview" style={{ '--bk-fill': 'var(--cyan-50)', '--bk-border': 'var(--cyan-600)', '--bk-sev': tone ? V6_SEV[tone].edge : 'transparent' }}>
            <span className="v6m-preview-l">미리보기 <span style={{ color: 'var(--cyan-700)', fontFamily: 'var(--ff-mono)', fontSize: 10 }}>cyan · 커스텀/메타 도메인</span></span>
            <span className={`v6m-pv-block shape-${shape}${tone ? ' sev-' + tone : ''}`}>
              <span className="bk-cap" style={{ '--bk-fill': 'var(--cyan-50)', '--bk-border': 'var(--cyan-600)' }} />
              <span className="v6m-pv-name">{fullName}</span>
              {tone && <span className={`bk-tag sev-${tone}`} style={{ position: 'static' }}>{V6_SEV[tone].label}</span>}
            </span>
          </div>
        </div>

        <div className="v6m-foot">
          <span className="v6m-foot-note"><V5I.check style={{ width: 14, height: 14 }} />추가 후 즉시 캔버스로 드래그 가능</span>
          <span className="spc" />
          <button className="btn-secondary" onClick={onClose}>취소</button>
          <button className={`btn-primary ${valid ? 'on' : ''}`} disabled={!valid} onClick={submit}>블록 추가</button>
        </div>
      </div>
    </div>
  );
}

function V6FolderModal({ onClose, onSubmit, customTree, defaultParent }) {
  const [name, setName] = useSMod('');
  const [parentId, setParentId] = useSMod(defaultParent || '');
  const folders = v6Folders(customTree);
  const clean = name.trim().replace(/\s+/g, '_');
  const valid = clean.length > 0;

  const submit = () => { if (valid) onSubmit({ name: clean, parentId: parentId || null }); };

  return (
    <div className="scrim" onClick={onClose}>
      <div className="v6modal" style={{ width: 440 }} onClick={(e) => e.stopPropagation()}>
        <div className="v6m-h">
          <div className="v6m-ic"><V6I.folderPlus /></div>
          <div className="v6m-htxt">
            <div className="v6m-eye">Custom · enrichment</div>
            <div className="v6m-t">폴더 추가</div>
            <div className="v6m-d">커스텀 블록이 많아질 때 임의 폴더로 그룹화합니다.</div>
          </div>
          <button className="v6m-x" onClick={onClose}><V5I.x style={{ width: 14, height: 14 }} /></button>
        </div>
        <div className="v6m-body">
          <div className="v6m-field">
            <span className="v6m-field-l">폴더명 <span className="req">*</span></span>
            <input className="v6m-input mono" autoFocus value={name}
              placeholder="oracle"
              onChange={(e) => setName(e.target.value)}
              onKeyDown={(e) => e.key === 'Enter' && submit()} />
          </div>
          <div className="v6m-field">
            <span className="v6m-field-l">대상 위치</span>
            <select className="v6m-input" value={parentId} onChange={(e) => setParentId(e.target.value)}>
              <option value="">커스텀 루트</option>
              {folders.map(f => (
                <option key={f.id} value={f.id}>{'\u00A0'.repeat(f.depth * 2)}{f.depth > 0 ? '└ ' : ''}{f.name}/</option>
              ))}
            </select>
          </div>
          <div className="v6m-preview">
            <span className="v6m-preview-l">미리보기</span>
            <span style={{ display: 'inline-flex', alignItems: 'center', gap: 8, color: 'var(--slate-700)' }}>
              <V6I.folderOpen style={{ width: 16, height: 16, color: 'var(--warn-500)' }} />
              <span style={{ fontWeight: 600 }}>{clean || 'folder'}</span>
            </span>
          </div>
        </div>
        <div className="v6m-foot">
          <span className="spc" />
          <button className="btn-secondary" onClick={onClose}>취소</button>
          <button className={`btn-primary ${valid ? 'on' : ''}`} disabled={!valid} onClick={submit}>폴더 추가</button>
        </div>
      </div>
    </div>
  );
}

Object.assign(window, { V6SchemaModal, V6FolderModal, v6Folders, V6_LEAF_TYPES });
