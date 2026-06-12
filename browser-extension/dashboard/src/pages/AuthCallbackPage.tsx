/**
 * `AuthCallbackPage` — landing page for the OAuth redirect.
 *
 * The server redirects to `${DASHBOARD_URL}/auth/callback#access_token=…`.
 * This page consumes the fragment, persists the token, and forwards the
 * user to the main app.
 */

import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useTranslation } from "react-i18next";

import { consumeTokensFromHash } from "../server-api";
import { useAuth } from "../hooks/useAuth";

export function AuthCallbackPage() {
  const { t } = useTranslation("common");
  const { refresh } = useAuth();
  const navigate = useNavigate();
  // Flag, not message: the text is resolved at render time so it follows
  // the active locale.
  const [tokenMissing, setTokenMissing] = useState(false);

  useEffect(() => {
    const token = consumeTokensFromHash();
    if (!token) {
      setTokenMissing(true);
      return;
    }
    void refresh().then(() => navigate("/", { replace: true }));
  }, [navigate, refresh]);

  if (tokenMissing) {
    return (
      <div style={{ padding: 32 }}>
        <h1>{t("login.failedTitle")}</h1>
        <p>{t("login.noToken")}</p>
      </div>
    );
  }
  return (
    <div style={{ padding: 32 }}>
      <p>{t("login.completing")}</p>
    </div>
  );
}
