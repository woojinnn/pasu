import { createBrowserRouter, RouterProvider } from "react-router-dom";
import { ExtensionProvider } from "./sdk-context";
import { AppShell } from "./shell/AppShell";
import { HomeOrOnboarding } from "./pages/HomeOrOnboarding";
import { EditorPage } from "./pages/EditorPage";
import { LibraryPage } from "./pages/LibraryPage";
import { AuditPage } from "./pages/AuditPage";
import { SettingsPage } from "./pages/SettingsPage";
import { ManifestEditor } from "./pages/manifest-editor";
import { SchemaViewer } from "./pages/schema-viewer";
import { RpcEndpointPage } from "./pages/rpc-endpoint";
import { OnboardingPage } from "./pages/onboarding";

// Standalone Vite app at localhost:5174 — BrowserRouter only.
// Extension-bundling is a future concern (M-5, deferred).
const router = createBrowserRouter([
  {
    path: "/",
    element: (
      <ExtensionProvider>
        <AppShell />
      </ExtensionProvider>
    ),
    children: [
      // Phase 7 codex carry-over I: cold-start onboarding redirect.
      // `HomeOrOnboarding` reads the configured endpoint URL and the
      // installed enriched schema; when both are empty it redirects
      // to `/onboarding`. Otherwise it renders `HomePage`.
      { index: true, element: <HomeOrOnboarding /> },
      { path: "editor", element: <EditorPage /> },
      { path: "library", element: <LibraryPage /> },
      { path: "audit", element: <AuditPage /> },
      { path: "settings", element: <SettingsPage /> },
      // Phase 7.2: per-action manifest authoring. Preview navigates to
      // `/schema?action=…` which is handled by SchemaViewer (Phase 7.3).
      { path: "manifests/:action", element: <ManifestEditor /> },
      // Phase 7.3: enriched cedarschema viewer. Reads `?action=<snake>`
      // and (optionally) `?fromPreview=true` from the URL.
      { path: "schema", element: <SchemaViewer /> },
      // Phase 7.4: policy-rpc endpoint settings page.
      { path: "rpc-endpoint", element: <RpcEndpointPage /> },
      // Phase 7.4: first-run onboarding wizard. The "show on cold
      // storage" redirect from "/" is deferred to a follow-up; for
      // now reach this page via direct navigation or a settings link.
      { path: "onboarding", element: <OnboardingPage /> },
    ],
  },
]);

export function AppRouter() {
  return <RouterProvider router={router} />;
}
