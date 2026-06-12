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
  useRef,
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
import { clearExtensionTokens, syncTokensFromExtensionStorage } from "../extension-bootstrap";

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

function logCurrentUserSyncFailure(err: unknown): void {
  console.warn(
    "[Dambi] sync current user to extension failed:",
    err instanceof Error ? err.message : String(err),
  );
}

export function AuthProvider({ children }: { children: ReactNode }) {
  const [user, setUser] = useState<Me | null>(null);
  const [isLoading, setLoading] = useState(true);
  const [error, setError] = useState<Error | null>(null);
  // 동시 다발 refresh 의 out-of-order 경쟁 방어: 토큰 변경 한 번에도 storage
  // 이벤트로 refresh 가 여러 번 겹친다. 매 호출에 세대 번호를 부여해 가장 최근
  // 호출만 user/loading 을 확정하게 한다. 없으면 늦게 끝난 옛 호출이 최신 결과를
  // 덮어써 user 가 null↔me 로 튀고(창 튕김), stale 토큰을 읽은 호출이 "다른
  // 계정"으로 확정되기도 한다.
  const refreshGen = useRef(0);

  const refresh = useCallback(async () => {
    const gen = ++refreshGen.current;
    const isLatest = () => gen === refreshGen.current;
    setLoading(true);
    setError(null);
    try {
      // 확장 컨텍스트에서는 SW 의 chrome.storage 토큰을 localStorage 로 먼저
      // 동기화한 뒤 /auth/me 를 호출한다. 안 그러면 새 토큰이 mirror 되기 전에
      // fetchMe 가 옛 토큰을 읽어 "다른 계정"으로 확정되는 race 가 난다.
      if (isExtensionContext()) {
        try {
          await syncTokensFromExtensionStorage();
        } catch {
          /* 동기화 실패 시 기존 localStorage 토큰으로 진행 */
        }
      }
      const me = await fetchMe();
      if (!isLatest()) return; // 더 최신 refresh 가 떴으면 이 결과는 버린다
      setUser(me);
      // Sync the SW's active-user discriminator so every per-user storage
      // key (`dashboard:policies:<id>`, `policy-selection:enabled-ids:<id>`,
      // …) reads/writes under the right namespace. Fire-and-forget — the
      // dashboard auth state must not depend on extension-local storage sync.
      if (me) {
        void setCurrentUser(me.user_id).catch(logCurrentUserSyncFailure);
      } else {
        void clearCurrentUser().catch(logCurrentUserSyncFailure);
      }
    } catch (e) {
      if (!isLatest()) return;
      if (e instanceof ServerError && e.isUnauthorized) {
        // fetchMe already cleared the token.
        setUser(null);
        void clearCurrentUser().catch(logCurrentUserSyncFailure);
      } else {
        setError(e instanceof Error ? e : new Error(String(e)));
      }
    } finally {
      if (isLatest()) setLoading(false);
    }
  }, []);

  // Initial probe + cross-tab sync. Listening on `storage` lets a
  // logout in one tab take effect in others.
  useEffect(() => {
    void refresh();
    const onStorage = (e: StorageEvent) => {
      if (e.key === "dambi_jwt") void refresh();
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
    void sendToSw("dambi-auth-sign-in")
      .then(() => syncTokensFromExtensionStorage())
      .then(() => refresh())
      .catch((e) => {
        setError(e instanceof Error ? e : new Error(String(e)));
        setLoading(false);
      });
  }, [refresh]);

  const logoutCb = useCallback(() => {
    // Clear the SW-owned token store (chrome.storage.local) FIRST and await it.
    // The SW is the shared source of truth the popup and every page sync from;
    // if we cleared the page tokens first, a concurrent storage-event refresh
    // would re-hydrate this page from the still-populated SW store (and re-set
    // `user`, so the page never redirects to /login). Only after the SW is
    // empty do we drop the page tokens + reset state.
    const finish = () => {
      serverLogout(); // clear THIS page's localStorage tokens
      void clearCurrentUser().catch(logCurrentUserSyncFailure);
      setUser(null);
    };
    if (isExtensionContext()) {
      // Clear BOTH stores page-side first (doesn't depend on the SW message
      // succeeding), then message the SW so it drops its in-memory cache and
      // the popup re-renders as signed-out.
      void clearExtensionTokens()
        .catch(() => {})
        .then(() =>
          sendToSw("dambi-auth-sign-out").catch((err) =>
            console.warn("[Dambi] SW sign-out failed:", err instanceof Error ? err.message : err),
          ),
        )
        .finally(finish);
    } else {
      finish();
    }
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
