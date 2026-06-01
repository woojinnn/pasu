import { createBrowserRouter, Navigate, RouterProvider } from "react-router-dom";

import { AppShell } from "./shell/AppShell";
import { HomePage } from "./pages/HomePage";
import { AuditPage } from "./pages/AuditPage";
import { HistoryPage } from "./pages/HistoryPage";
import { MonitoringPage } from "./pages/MonitoringPage";
import { EditorPage } from "./pages/EditorPage";
import { SimulationPage } from "./pages/SimulationPage";
import { LoginPage } from "./pages/LoginPage";
import { AuthCallbackPage } from "./pages/AuthCallbackPage";
import { RequireAuth } from "./auth/RequireAuth";

/**
 * Top-level routes. Home / Audit / History wired today;
 * Editor / Simulation / Monitoring land in later phases (NavRail shows
 * them as disabled placeholders).
 */
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
          { path: "editor", element: <EditorPage /> },
          { path: "simulation", element: <SimulationPage /> },
          { path: "monitoring", element: <MonitoringPage /> },
          { path: "audit", element: <AuditPage /> },
          { path: "history", element: <HistoryPage /> },
          { path: "*", element: <Navigate to="/" replace /> },
        ],
      },
    ],
  },
]);

export function AppRouter() {
  return <RouterProvider router={router} />;
}
