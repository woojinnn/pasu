import { createBrowserRouter, Navigate, RouterProvider } from "react-router-dom";

import { AppShell } from "./shell/AppShell";
import { HomePage } from "./pages/HomePage";
import { LoginPage } from "./pages/LoginPage";
import { AuthCallbackPage } from "./pages/AuthCallbackPage";
import { RequireAuth } from "./auth/RequireAuth";

/**
 * Top-level routes. Phase 0 scaffolds the auth flow + Home shell only;
 * Editor / Simulation / Monitoring / Audit / History pages land in
 * later phases as their APIs come online.
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
          // TODO Phase 1+: editor, simulation, monitoring, audit, history, settings
          { path: "*", element: <Navigate to="/" replace /> },
        ],
      },
    ],
  },
]);

export function AppRouter() {
  return <RouterProvider router={router} />;
}
