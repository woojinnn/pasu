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
  fetchMe,
  logout as serverLogout,
  startGoogleLogin,
  type Me,
} from "../server-api";

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
    } catch (e) {
      if (e instanceof ServerError && e.isUnauthorized) {
        // fetchMe already cleared the token.
        setUser(null);
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
      if (e.key === "scopeball_jwt") void refresh();
    };
    window.addEventListener("storage", onStorage);
    return () => window.removeEventListener("storage", onStorage);
  }, [refresh]);

  const logoutCb = useCallback(() => {
    serverLogout();
    setUser(null);
  }, []);

  return createElement(
    AuthContext.Provider,
    {
      value: {
        user,
        isLoading,
        error,
        login: startGoogleLogin,
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
