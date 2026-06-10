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

import {
  Navigate,
  RouterProvider,
  createBrowserRouter,
  createHashRouter,
} from "react-router-dom";

import { isExtensionContext } from "./env";
import { AuthProvider } from "./hooks/useAuth";
import { RequireAuth } from "./RequireAuth";
import { AppShell } from "./shell/AppShell";

import { LoginPage } from "./pages/LoginPage";
import { AuthCallbackPage } from "./pages/AuthCallbackPage";
import { HomePage } from "./pages/HomePage";
import { EditorListPage } from "./pages/editor/EditorListPage";
import { EditorDetailPage } from "./pages/editor/EditorDetailPage";
import { SimulationPage } from "./pages/SimulationPage";
import { MonitoringPage } from "./pages/MonitoringPage";
import { HistoryPage } from "./pages/HistoryPage";
import { MarketPage } from "./pages/MarketPage";
import { MarketDetailPage } from "./pages/MarketDetailPage";
import { SettingsPage } from "./pages/SettingsPage";
import { ProfilePage } from "./pages/ProfilePage";

// On an extension page the URL is `…/options.html` (a real file, no dev
// server rewriting unknown paths to index.html), so path-based routing finds
// no match → blank screen. Hash routing (`…/options.html#/editor`) renders
// and survives reloads. Standalone dev keeps clean path-based URLs.
const createRouter = isExtensionContext() ? createHashRouter : createBrowserRouter;

const router = createRouter([
  { path: "/login", element: <LoginPage /> },
  { path: "/auth/callback", element: <AuthCallbackPage /> },
  { path: "/settings", element: <SettingsPage /> },
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
          { path: "editor/new", element: <Navigate to="/editor" replace /> },
          { path: "editor/:id", element: <EditorDetailPage /> },
          { path: "simulation", element: <SimulationPage /> },
          { path: "monitoring", element: <MonitoringPage /> },
          { path: "history", element: <HistoryPage /> },
          { path: "market", element: <MarketPage /> },
          { path: "market/:slug", element: <MarketDetailPage /> },
          { path: "profile", element: <ProfilePage /> },
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
