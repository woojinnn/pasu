import { EditorListPageV2 } from "./v2/EditorListPageV2";

/**
 * Router-exposed entry for `/editor`. The v2 list is the only path.
 */
export function EditorListPage() {
  return <EditorListPageV2 />;
}
