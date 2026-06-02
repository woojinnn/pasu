import {
  displayParam,
  getGlossEntry,
  OPS_BY_FIELDKIND,
  OP_SYMBOL,
  type FieldKind,
  type Op,
  type PredicateValue,
} from "../schema";
import type { EditorAction, EditorState } from "../reducer";
import type { HatNode, LogicNode, NodeId, PredicateNode } from "../types";

/**
 * Right-side inspector for the selected block.
 *
 * Three modes depending on the node type:
 *   - HatNode      → policy header (name, action, effect, denyMessage)
 *   - LogicNode    → op switcher (AND/OR/NOT), guard label, enabled toggle
 *   - PredicateNode→ op picker (op set scoped by fieldKind),
 *                    value input (typed by fieldKind), absence policy,
 *                    guard label, user-copy (headline/plain), parent
 *                    picker for connecting drafts, delete button.
 *
 * Edits dispatch UPDATE_* actions; the reducer rebuilds nodes immutably
 * so React stays happy with reference equality.
 */
export interface InspectorProps {
  state: EditorState;
  dispatch: (a: EditorAction) => void;
}

export function Inspector({ state, dispatch }: InspectorProps) {
  const sel = state.selectedId ? state.doc.nodes[state.selectedId] : null;

  if (!sel) {
    return (
      <aside className="v7-inspector">
        <div className="ins-empty">
          <p>왼쪽 캔버스에서 블록을 선택하세요.</p>
          <p className="hint">
            <strong>판</strong>·드래그: shift + 좌클릭 또는 휠 클릭<br />
            <strong>확대</strong>: ctrl + 휠
          </p>
        </div>
      </aside>
    );
  }

  if (sel.type === "hat") return <HatInspector hat={sel} state={state} dispatch={dispatch} />;
  if (sel.type === "logic") return <LogicInspector node={sel} state={state} dispatch={dispatch} />;
  return <PredicateInspector node={sel} state={state} dispatch={dispatch} />;
}

// ── hat ────────────────────────────────────────────────────────────────

function HatInspector({
  hat,
  state,
  dispatch,
}: {
  hat: HatNode;
  state: EditorState;
  dispatch: (a: EditorAction) => void;
}) {
  return (
    <aside className="v7-inspector">
      <div className="ins-head">
        <span className="ins-kind">정책 헤더</span>
        <span className="ins-id">{hat.id}</span>
      </div>
      <label className="ins-row">
        <span>정책 이름</span>
        <input
          value={state.doc.policyName}
          onChange={(e) => dispatch({ type: "SET_POLICY_NAME", name: e.target.value })}
        />
      </label>
      <label className="ins-row">
        <span>액션 (Cedar action id)</span>
        <input
          value={hat.action}
          onChange={(e) => dispatch({ type: "SET_HAT", action: e.target.value })}
          placeholder="Amm::Swap"
        />
      </label>
      <label className="ins-row">
        <span>판정</span>
        <select
          value={hat.effect}
          onChange={(e) => dispatch({ type: "SET_HAT", effect: e.target.value as "permit" | "deny" })}
        >
          <option value="permit">permit (허용 기준)</option>
          <option value="deny">deny (차단 기준)</option>
        </select>
      </label>
      <label className="ins-row">
        <span>거부 메시지</span>
        <textarea
          value={state.doc.denyMessage ?? ""}
          onChange={(e) => dispatch({ type: "SET_DENY_MESSAGE", message: e.target.value })}
          rows={2}
          placeholder="정책 위반 시 사용자에게 보일 메시지"
        />
      </label>
    </aside>
  );
}

// ── logic ──────────────────────────────────────────────────────────────

function LogicInspector({
  node,
  state,
  dispatch,
}: {
  node: LogicNode;
  state: EditorState;
  dispatch: (a: EditorAction) => void;
}) {
  const isRoot = node.id === state.doc.rootId;
  return (
    <aside className="v7-inspector">
      <div className="ins-head">
        <span className="ins-kind">논리 블록</span>
        <span className="ins-id">{node.id}</span>
      </div>
      <label className="ins-row">
        <span>연산자</span>
        <div className="ins-op-row">
          {(["AND", "OR", "NOT"] as const).map((op) => (
            <button
              key={op}
              className={`op-pick${node.op === op ? " active" : ""}`}
              onClick={() => dispatch({ type: "UPDATE_LOGIC", nodeId: node.id, patch: { op } })}
            >
              {op}
            </button>
          ))}
        </div>
      </label>
      <label className="ins-row">
        <span>가드 ID (옵션)</span>
        <input
          value={node.guardId ?? ""}
          onChange={(e) =>
            dispatch({ type: "UPDATE_LOGIC", nodeId: node.id, patch: { guardId: e.target.value || undefined } })
          }
          placeholder="s1, slippageGuard, …"
        />
      </label>
      <label className="ins-row">
        <span>가드 라벨</span>
        <input
          value={node.label ?? ""}
          onChange={(e) =>
            dispatch({ type: "UPDATE_LOGIC", nodeId: node.id, patch: { label: e.target.value || undefined } })
          }
          placeholder="슬리피지 가드"
        />
      </label>
      <fieldset className="ins-fieldset">
        <legend>사용자 안내문</legend>
        <input
          placeholder="요약 (Inspector 헤드라인)"
          value={node.userCopy?.headline ?? ""}
          onChange={(e) =>
            dispatch({
              type: "UPDATE_LOGIC",
              nodeId: node.id,
              patch: { userCopy: { ...node.userCopy, headline: e.target.value } },
            })
          }
        />
        <textarea
          rows={2}
          placeholder="평이한 설명"
          value={node.userCopy?.plain ?? ""}
          onChange={(e) =>
            dispatch({
              type: "UPDATE_LOGIC",
              nodeId: node.id,
              patch: { userCopy: { ...node.userCopy, plain: e.target.value } },
            })
          }
        />
      </fieldset>
      <label className="ins-row chk">
        <input
          type="checkbox"
          checked={node.enabled !== false}
          onChange={(e) => dispatch({ type: "UPDATE_LOGIC", nodeId: node.id, patch: { enabled: e.target.checked } })}
        />
        <span>활성화</span>
      </label>
      {!isRoot && (
        <div className="ins-row">
          <span />
          <button
            className="btn-danger"
            onClick={() => dispatch({ type: "DELETE", nodeId: node.id })}
          >
            블록 삭제 (자손 포함)
          </button>
        </div>
      )}
    </aside>
  );
}

// ── predicate ──────────────────────────────────────────────────────────

function PredicateInspector({
  node,
  state,
  dispatch,
}: {
  node: PredicateNode;
  state: EditorState;
  dispatch: (a: EditorAction) => void;
}) {
  const ops: Op[] = OPS_BY_FIELDKIND[node.fieldKind] ?? [];

  const updateValue = (raw: string) => {
    if (raw === "" && (node.op === "isTrue" || node.op === "isFalse")) {
      dispatch({ type: "UPDATE_PREDICATE", nodeId: node.id, patch: { value: null } });
      return;
    }
    dispatch({ type: "UPDATE_PREDICATE", nodeId: node.id, patch: { value: coerceValue(raw, node.fieldKind) } });
  };

  const valueText = node.value?.text ?? "";
  const valueless = node.op === "isTrue" || node.op === "isFalse" || node.op === "isEmpty";

  // Logic-block children of root suitable as a parent. The user can re-parent
  // a draft predicate by picking one here; CONNECT also clears `float`.
  const parents = listConnectableParents(state, node.id);

  const entry = getGlossEntry(node.param);
  const desc = entry
    ? state.doc.locale === "ko" ? entry.desc.ko : entry.desc.en
    : null;

  return (
    <aside className="v7-inspector">
      <div className="ins-head">
        <span className="ins-kind">술어 블록</span>
        <span className="ins-id">{node.id}</span>
      </div>
      <div className="ins-row read">
        <span>필드</span>
        <code title={node.param}>{displayParam(node.param, state.doc.locale)}</code>
        {desc && <p className="ins-desc">{desc}</p>}
      </div>
      <div className="ins-row read">
        <span>경로</span>
        <code className="mono small">{node.param}</code>
      </div>
      <label className="ins-row">
        <span>비교 연산자</span>
        <select
          value={node.op}
          onChange={(e) =>
            dispatch({ type: "UPDATE_PREDICATE", nodeId: node.id, patch: { op: e.target.value as Op } })
          }
        >
          {ops.map((op) => (
            <option key={op} value={op}>
              {op} ({OP_SYMBOL[op]})
            </option>
          ))}
        </select>
      </label>
      {!valueless && (
        <label className="ins-row">
          <span>비교값</span>
          <input
            value={valueText}
            onChange={(e) => updateValue(e.target.value)}
            placeholder="100, @meta.from, 0xabc…"
          />
        </label>
      )}
      {node.param.startsWith("enrichment.") && (
        <label className="ins-row">
          <span>값 부재 시</span>
          <select
            value={node.absence ?? "treatAsFalse"}
            onChange={(e) =>
              dispatch({
                type: "UPDATE_PREDICATE",
                nodeId: node.id,
                patch: { absence: e.target.value as "treatAsFalse" | "treatAsTrue" },
              })
            }
          >
            <option value="treatAsFalse">treatAsFalse (안전 기본)</option>
            <option value="treatAsTrue">treatAsTrue (없으면 통과)</option>
          </select>
        </label>
      )}
      <label className="ins-row">
        <span>가드 ID</span>
        <input
          value={node.guardId ?? ""}
          onChange={(e) =>
            dispatch({
              type: "UPDATE_PREDICATE",
              nodeId: node.id,
              patch: { guardId: e.target.value || undefined },
            })
          }
        />
      </label>
      <label className="ins-row">
        <span>가드 라벨</span>
        <input
          value={node.label ?? ""}
          onChange={(e) =>
            dispatch({
              type: "UPDATE_PREDICATE",
              nodeId: node.id,
              patch: { label: e.target.value || undefined },
            })
          }
        />
      </label>
      <label className="ins-row">
        <span>메모</span>
        <textarea
          rows={2}
          value={node.note ?? ""}
          onChange={(e) =>
            dispatch({
              type: "UPDATE_PREDICATE",
              nodeId: node.id,
              patch: { note: e.target.value || undefined },
            })
          }
        />
      </label>
      <label className="ins-row chk">
        <input
          type="checkbox"
          checked={node.enabled !== false}
          onChange={(e) =>
            dispatch({ type: "UPDATE_PREDICATE", nodeId: node.id, patch: { enabled: e.target.checked } })
          }
        />
        <span>활성화</span>
      </label>

      <fieldset className="ins-fieldset">
        <legend>부모 블록</legend>
        {node.float ? (
          <>
            <p className="hint">미연결 상태입니다. 부모를 선택해 연결하세요.</p>
            <select
              defaultValue=""
              onChange={(e) => {
                const pid = e.target.value;
                if (pid) dispatch({ type: "CONNECT", childId: node.id, parentId: pid });
              }}
            >
              <option value="">— 부모 선택 —</option>
              {parents.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.label}
                </option>
              ))}
            </select>
          </>
        ) : (
          <button
            className="btn-secondary"
            onClick={() => dispatch({ type: "DISCONNECT", nodeId: node.id })}
          >
            트리에서 분리 (draft으로 이동)
          </button>
        )}
      </fieldset>

      <div className="ins-row">
        <span />
        <button className="btn-danger" onClick={() => dispatch({ type: "DELETE", nodeId: node.id })}>
          블록 삭제
        </button>
      </div>
    </aside>
  );
}

// ── helpers ────────────────────────────────────────────────────────────

/** Mirror of `PredicateBlock`'s coerceValue — kept duplicated to avoid
 *  a circular import through doc.ts. */
function coerceValue(raw: string, fk: FieldKind): PredicateValue {
  if (raw.startsWith("@")) return { kind: "ref", text: raw };
  if (fk === "primitive.Long" || fk === "primitive.decimal") {
    return { kind: "num", text: raw };
  }
  if (fk === "primitive.Bool") {
    return { kind: "bool", text: raw.toLowerCase() === "true" ? "true" : "false" };
  }
  return { kind: "str", text: raw };
}

function listConnectableParents(state: EditorState, selfId: NodeId): Array<{ id: NodeId; label: string }> {
  const out: Array<{ id: NodeId; label: string }> = [];
  for (const n of Object.values(state.doc.nodes)) {
    if (n.id === selfId) continue;
    if (n.type !== "logic") continue;
    const label = `${n.op}${n.label ? ` · ${n.label}` : ""}${n.guardId ? ` (${n.guardId})` : ""}`;
    out.push({ id: n.id, label });
  }
  return out;
}
