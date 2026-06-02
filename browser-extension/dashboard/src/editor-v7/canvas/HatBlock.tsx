import type { HatNode } from "../types";

/**
 * The hat block — sits at the top of every policy. Shows the effect
 * (permit / deny) and action (e.g. `Amm::Swap`). Not draggable —
 * anchored at the top of the tree column.
 */
export interface HatBlockProps {
  node: HatNode;
  selected: boolean;
  onSelect: () => void;
}

export function HatBlock({ node, selected, onSelect }: HatBlockProps) {
  return (
    <div
      className={`v7-block v7-hat ${node.effect}${selected ? " selected" : ""}`}
      onClick={(e) => {
        e.stopPropagation();
        onSelect();
      }}
    >
      <span className="effect-pill">{node.effect === "permit" ? "허용" : "차단"}</span>
      <span className="action-label">{node.action}</span>
    </div>
  );
}
