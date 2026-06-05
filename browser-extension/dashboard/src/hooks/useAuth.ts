/**
 * `useAuth` — React Context + hook for the dashboard's auth state.
 *
 * Single source of truth for "who is the current user". On mount it
 * resolves the stored JWT against `/auth/me`; pages can call
 * `useAuth()` to read `{ user, isLoading, error, login, logout }`
 * without thinking about localStorage or fetch.
 *
 * `<AuthProvider>` wraps the router; protected pages can early-return
 * a redirect when `user` is `null` and `isLoading` is `false`.
 */

import {
  createContext,
  createElement,
  useCallback,
  useContext,
  useEffect,
  useState,
  type ReactNode,
} from "react";

import {
  ServerError,
  clearCurrentUser,
  fetchMe,
  logout as serverLogout,
  setCurrentUser,
  startGoogleLogin,
  type Me,
} from "../server-api";
import { isExtensionContext } from "../env";
import { sendToSw } from "../server-api/sw-bridge";
import { syncTokensFromExtensionStorage } from "../extension-bootstrap";

export interface AuthContextValue {
  /** The logged-in user, or `null` when logged out / not yet resolved. */
  user: Me | null;
  /** True until the initial `/auth/me` call settles. UI should render a
   * spinner / pass-through state during this window so it doesn't
   * flash the login page for already-logged-in users. */
  isLoading: boolean;
  /** Non-401 errors from the auth check. 401 is handled internally by
   * clearing the token and surfacing `user = null`. */
  error: Error | null;
  /** Kick off the Google OAuth flow (full-page navigation). */
  login: () => void;
  /** Drop tokens locally and reset state. */
  logout: () => void;
  /** Re-run `/auth/me`. Pages call this after consuming tokens from a
   * callback URL to force the provider to pick the new token up. */
  refresh: () => Promise<void>;
}

const AuthContext = createContext<AuthContextValue | null>(null);

export function AuthProvider({ children }: { children: ReactNode }) {
  const [user, setUser] = useState<Me | null>(null);
  const [isLoading, setLoading] = useState(true);
  const [error, setError] = useState<Error | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const me = await fetchMe();
      setUser(me);
      // Sync the SW's active-user discriminator so every per-user storage
      // key (`dashboard:policies:<id>`, `policy-selection:enabled-ids:<id>`,
      // …) reads/writes under the right namespace. Fire-and-forget — the
      // bridge fails soft when the extension isn't installed.
      if (me) {
        void setCurrentUser(me.user_id);
      } else {
        void clearCurrentUser();
      }
    } catch (e) {
      if (e instanceof ServerError && e.isUnauthorized) {
        // fetchMe already cleared the token.
        setUser(null);
        void clearCurrentUser();
      } else {
        setError(e instanceof Error ? e : new Error(String(e)));
      }
    } finally {
      setLoading(false);
    }
  }, []);

  // Initial probe + cross-tab sync. Listening on `storage` lets a
  // logout in one tab take effect in others.
  useEffect(() => {
    void refresh();
    const onStorage = (e: StorageEvent) => {
      if (e.key === "pasu_jwt") void refresh();
    };
    window.addEventListener("storage", onStorage);
    return () => window.removeEventListener("storage", onStorage);
  }, [refresh]);

  const login = useCallback(() => {
    if (!isExtensionContext()) {
      startGoogleLogin();
      return;
    }
    // In the extension a full-page nav to /auth/google can't round-trip — the
    // token comes back to the SW's chromiumapp.org redirect, not this page.
    // Drive the SW's launchWebAuthFlow (it stores the token in chrome.storage),
    // then mirror it into localStorage and re-resolve /auth/me. The dashboard
    // page itself never navigates away.
    setLoading(true);
    void sendToSw("pasu-auth-sign-in")
      .then(() => syncTokensFromExtensionStorage())
      .then(() => refresh())
      .catch((e) => {
        setError(e instanceof Error ? e : new Error(String(e)));
        setLoading(false);
      });
  }, [refresh]);

  const logoutCb = useCallback(() => {
    serverLogout();
    void clearCurrentUser();
    setUser(null);
  }, []);

  return createElement(
    AuthContext.Provider,
    {
      value: {
        user,
        isLoading,
        error,
        login,
        logout: logoutCb,
        refresh,
      },
    },
    children,
  );
}

/** Convenience consumer. Throws when used outside `<AuthProvider>` —
 * caught by React's error boundary, gives a clear stack instead of
 * silently returning `null`. */
export function useAuth(): AuthContextValue {
  const ctx = useContext(AuthContext);
  if (!ctx) throw new Error("useAuth must be used inside <AuthProvider>");
  return ctx;
}
