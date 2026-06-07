import { EditorDetailPageV2 } from "./v2/EditorDetailPageV2";

/**
 * Router-exposed entry for `/editor/:id`. The v2 detail view is the
 * only path.
 */
export function EditorDetailPage() {
  return <EditorDetailPageV2 />;
}
