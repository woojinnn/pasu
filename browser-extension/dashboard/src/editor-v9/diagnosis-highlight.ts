/**
 * Denial-diagnosis highlight: red-box the culprit blocks the blame walker
 * pinpointed, keyed by structural IR path → Blockly block id (the `pathMap`
 * the pure `pathToBlockId` combiner produced from irToWorkspace's identity map).
 *
 * The styling is a single CSS class (`diagnosis-culprit`) toggled on each
 * culprit block's SVG root; the visual is defined in `diagnosis-highlight.css`.
 */

import * as Blockly from "blockly";

const CULPRIT_CLASS = "diagnosis-culprit";

/** Clear any prior culprit styling + warning text from all blocks. */
export function clearCulprits(ws: Blockly.WorkspaceSvg): void {
  for (const b of ws.getAllBlocks(false)) {
    const block = b as Blockly.BlockSvg;
    const root = block.getSvgRoot?.();
    if (root) Blockly.utils.dom.removeClass(root, CULPRIT_CLASS);
    block.setWarningText?.(null);
  }
}

/** Red-box the blocks at the given IR paths and (optionally) annotate each with
 *  a short note. Clears prior highlights first, then centers on the first
 *  resolvable culprit so the user is taken straight to it. */
export function applyCulprits(
  ws: Blockly.WorkspaceSvg,
  pathMap: Map<string, string>,
  paths: string[],
  note?: (path: string) => string | null,
): void {
  clearCulprits(ws);
  for (const p of paths) {
    const id = pathMap.get(p);
    if (!id) continue;
    const block = ws.getBlockById(id) as Blockly.BlockSvg | null;
    const root = block?.getSvgRoot();
    if (root) Blockly.utils.dom.addClass(root, CULPRIT_CLASS);
    const text = note?.(p);
    if (block && text) block.setWarningText(text);
  }
  const first = paths.map((p) => pathMap.get(p)).find(Boolean);
  if (first) ws.centerOnBlock(first, true);
}
