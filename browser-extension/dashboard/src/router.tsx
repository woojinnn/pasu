/**
 * Dashboard router — single SPA mounted at `/`.
 *
 * Auth flow:
 *   `/login`            — public, kicks the Google OAuth redirect.
 *   `/auth/callback`    — public, parses the token from the URL hash.
 *   everything else     — `<RequireAuth>` bounces anonymous users to /login.
 *
 * Shell: `<AppShell>` renders the collapsible nav rail; pages render
 * inside its `<Outlet />`. Each page owns its own `<Topbar />` (crumb
 * varies per route).
 */

import { Navigate, RouterProvider, createBrowserRouter } from "react-router-dom";

import { AuthProvider } from "./hooks/useAuth";
import { RequireAuth } from "./RequireAuth";
import { AppShell } from "./shell/AppShell";

import { LoginPage } from "./pages/LoginPage";
import { AuthCallbackPage } from "./pages/AuthCallbackPage";
import { HomePage } from "./pages/HomePage";
import { EditorListPage } from "./pages/editor/EditorListPage";
import { EditorNewPage } from "./pages/editor/EditorNewPage";
import { EditorDetailPage } from "./pages/editor/EditorDetailPage";
import { SimulationPage } from "./pages/SimulationPage";
import { MonitoringPage } from "./pages/MonitoringPage";
import { HistoryPage } from "./pages/HistoryPage";

const router = createBrowserRouter([
  { path: "/login", element: <LoginPage /> },
  { path: "/auth/callback", element: <AuthCallbackPage /> },
  {
    path: "/",
    element: <RequireAuth />,
    children: [
      {
        path: "",
        element: <AppShell />,
        children: [
          { index: true, element: <HomePage /> },
          { path: "editor", element: <EditorListPage /> },
          { path: "editor/new", element: <EditorNewPage /> },
          { path: "editor/:id", element: <EditorDetailPage /> },
          { path: "simulation", element: <SimulationPage /> },
          { path: "monitoring", element: <MonitoringPage /> },
          { path: "history", element: <HistoryPage /> },
          { path: "*", element: <Navigate to="/" replace /> },
        ],
      },
    ],
  },
]);

export function AppRouter() {
  return (
    <AuthProvider>
      <RouterProvider router={router} />
    </AuthProvider>
  );
}
