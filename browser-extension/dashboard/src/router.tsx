import { createBrowserRouter, RouterProvider } from "react-router-dom";
import { ExtensionProvider } from "./sdk-context";
import { AppShell } from "./shell/AppShell";
import { HomePage } from "./pages/HomePage";
import { EditorPage } from "./pages/EditorPage";
import { LibraryPage } from "./pages/LibraryPage";
import { AuditPage } from "./pages/AuditPage";
import { SettingsPage } from "./pages/SettingsPage";

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
      { index: true, element: <HomePage /> },
      { path: "editor", element: <EditorPage /> },
      { path: "library", element: <LibraryPage /> },
      { path: "audit", element: <AuditPage /> },
      { path: "settings", element: <SettingsPage /> },
    ],
  },
]);

export function AppRouter() {
  return <RouterProvider router={router} />;
}
