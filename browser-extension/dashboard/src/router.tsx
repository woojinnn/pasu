import { Navigate, createBrowserRouter, RouterProvider } from "react-router-dom";

import { ExtensionProvider } from "./sdk-context";
import { AppShell } from "./shell/AppShell";
import { HomeOrOnboarding } from "./pages/HomeOrOnboarding";
import { LibraryPage } from "./pages/LibraryPage";
import { AuditPage } from "./pages/AuditPage";
import { SettingsPage } from "./pages/SettingsPage";
import { SchemaViewer } from "./pages/schema-viewer";
import { RpcEndpointPage } from "./pages/rpc-endpoint";
import { OnboardingPage } from "./pages/onboarding";

import { AuthProvider } from "./hooks/useAuth";
import { RequireAuth } from "./RequireAuth";
import { ServerLayout } from "./ServerLayout";
import { LoginPage } from "./pages/LoginPage";
import { AuthCallbackPage } from "./pages/AuthCallbackPage";
import { WalletsPage } from "./pages/WalletsPage";
import { WalletDetailPage } from "./pages/WalletDetailPage";
import { ActivityPage } from "./pages/ActivityPage";
import { MePage } from "./pages/MePage";
import { TransactionsPage } from "./pages/TransactionsPage";
import { PoliciesPage } from "./pages/PoliciesPage";
import { TokensPage } from "./pages/TokensPage";

// Standalone Vite app at localhost:5174 — BrowserRouter only.
// Extension-bundling is a future concern (M-5, deferred).
const router = createBrowserRouter([
  // Public auth routes — sit outside the extension shell so the user can
  // log in without a working extension connection.
  { path: "/login", element: <LoginPage /> },
  { path: "/auth/callback", element: <AuthCallbackPage /> },

  // Server-aware (policy-rpc) subtree. Gated by `<RequireAuth>` so
  // anonymous users bounce to /login. `<ServerLayout>` provides the
  // shared nav bar across all /server/* pages.
  {
    path: "/server",
    element: <RequireAuth />,
    children: [
      {
        path: "",
        element: <ServerLayout />,
        children: [
          { index: true, element: <Navigate to="/server/wallets" replace /> },
          { path: "wallets", element: <WalletsPage /> },
          { path: "wallets/:address", element: <WalletDetailPage /> },
          { path: "transactions", element: <TransactionsPage /> },
          { path: "policies", element: <PoliciesPage /> },
          { path: "tokens", element: <TokensPage /> },
          { path: "activity", element: <ActivityPage /> },
          { path: "me", element: <MePage /> },
        ],
      },
    ],
  },

  // Existing extension-SDK shell — unchanged.
  {
    path: "/",
    element: (
      <ExtensionProvider>
        <AppShell />
      </ExtensionProvider>
    ),
    children: [
      { index: true, element: <HomeOrOnboarding /> },
      { path: "policies", element: <LibraryPage /> },
      { path: "audit", element: <AuditPage /> },
      { path: "settings", element: <SettingsPage /> },
      { path: "schema", element: <SchemaViewer /> },
      { path: "rpc-endpoint", element: <RpcEndpointPage /> },
      { path: "onboarding", element: <OnboardingPage /> },
    ],
  },
]);

// `AuthProvider` wraps the entire router so both the public (/login,
// /auth/callback) and gated (/server/*) routes share one auth context.
// Existing extension routes don't need auth and won't read from it.
export function AppRouter() {
  return (
    <AuthProvider>
      <RouterProvider router={router} />
    </AuthProvider>
  );
}
