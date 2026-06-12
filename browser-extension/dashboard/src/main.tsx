import React from "react";
import { createRoot } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import "./tokens.css";
import { AppRouter } from "./router";
import { bootstrapExtensionEnv } from "./extension-bootstrap";
import { migratePasuRenameLocalStorage } from "./pasu-rename-storage-migration";
import { initI18n } from "./i18n";

const queryClient = new QueryClient();

// One-time scopeball_* → pasu_* localStorage migration. Runs synchronously
// before bootstrapExtensionEnv() and the first render so the renamed JWT /
// server-url / market-locale keys are populated before anything reads them —
// a returning user keeps their session and preferences across the rename.
migratePasuRenameLocalStorage();

// i18n reads the persisted locale from localStorage, so it must init after
// the key migration above and before the first render below.
initI18n();

const root = document.getElementById("root");
if (!root) throw new Error("Missing #root mount node");

// On an extension page (options.html), hydrate the SW-owned JWT from
// chrome.storage into localStorage BEFORE the first render so useAuth's
// initial /auth/me check is authenticated. `.finally` guarantees the app
// renders whether or not the sync succeeds — never a blank page. No-op in
// the standalone dev build (served over http at localhost:5173).
void bootstrapExtensionEnv().finally(() => {
  createRoot(root).render(
    <React.StrictMode>
      <QueryClientProvider client={queryClient}>
        <AppRouter />
      </QueryClientProvider>
    </React.StrictMode>,
  );
});
