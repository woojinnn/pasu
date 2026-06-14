/**
 * Google OAuth flow inside the browser extension.
 *
 * Uses `chrome.identity.launchWebAuthFlow` — Chrome opens a popup window
 * pointed at our server's `GET /auth/google`, lets the user complete
 * Google sign-in, and then captures the final redirect URL when the
 * server bounces back. We pull the JWT out of the URL fragment and
 * stash it via `tokenStore`.
 *
 * Why `launchWebAuthFlow` and not just opening a new tab?
 * - It handles closing the popup automatically.
 * - It returns the final redirect URL to us, even if the URL is not
 *   reachable by the browser (we control the redirect target).
 *
 * The redirect target is the dashboard's `/auth/callback` URL by
 * default. The server sends `#access_token=…&refresh_token=…` in the
 * fragment; `launchWebAuthFlow` reports that URL back to us, and we
 * never actually navigate the dashboard tab.
 */

import Browser from "webextension-polyfill";

import { setTokens } from "./tokenStore";
import { getServerBaseUrl } from "./client";

/**
 * Kick off the OAuth flow. Resolves with the user's email on success,
 * rejects with an Error on failure / user cancellation.
 *
 * Note: `chrome.identity.launchWebAuthFlow` requires the manifest to
 * declare the `identity` permission. Firefox supports the same API via
 * `browser.identity.launchWebAuthFlow`; `webextension-polyfill` papers
 * over the difference.
 */
export async function startGoogleLogin(): Promise<{ access: string; refresh: string | null }> {
  const identity = (
    Browser as unknown as {
      identity: {
        getRedirectURL(): string;
        launchWebAuthFlow(opts: { url: string; interactive: boolean }): Promise<string>;
      };
    }
  ).identity;

  // The server must bounce the token back to THIS exact URL — launchWebAuthFlow
  // only resolves on a redirect to `https://<ext-id>.chromiumapp.org/`. That
  // string must be byte-for-byte in the server's OAUTH_ALLOWED_REDIRECT_URIS
  // (trailing slash included); log it so it can be copied into the allowlist.
  const redirectUri = identity.getRedirectURL();
  console.log("[dambi] OAuth redirect_uri (allowlist this exactly):", redirectUri);

  const url = `${getServerBaseUrl()}/auth/google?redirect_uri=${encodeURIComponent(redirectUri)}`;
  // launchWebAuthFlow 가 던지는 에러(대표적으로 "The user did not approve
  // access.")는 원인이 모호하다 — 사용자가 정말 동의를 닫았을 수도, 로드된 확장
  // ID 가 서버 allowlist 의 chromiumapp.org URL 과 달라 콜백이 이 확장으로 못
  // 돌아왔을 수도 있다. 둘을 한눈에 가리려면 실제 redirect_uri(확장 ID 포함)를
  // 에러에 박아 화면까지 올려준다 — 그래야 "ID 가 다른지/allowlist 누락인지"를
  // 콘솔을 안 봐도 진단할 수 있다.
  let redirectUrl: string;
  try {
    redirectUrl = await identity.launchWebAuthFlow({
      url,
      interactive: true,
    });
  } catch (err) {
    const cause = err instanceof Error ? err.message : String(err);
    throw new Error(
      `로그인 실패 (redirect_uri=${redirectUri}). ` +
        `이 URL 이 chrome://extensions 의 확장 ID 와 일치하고, 서버 ` +
        `OAUTH_ALLOWED_REDIRECT_URIS 에 (끝 슬래시 포함) 정확히 등록됐는지 ` +
        `확인하세요. 원인: ${cause}`,
    );
  }

  if (!redirectUrl) {
    throw new Error("OAuth flow returned no redirect URL");
  }
  const { access, refresh } = parseTokensFromUrl(redirectUrl);
  if (!access) {
    // launchWebAuthFlow 가 토큰 fragment 없이 resolve 됐다 — 흔히 서버가
    // "redirect_uri not allowed" 같은 에러 페이지로 바운스한 경우다. 받은 URL 을
    // 그대로 노출해 서버 쪽 거부인지(allowlist 누락) 바로 보이게 한다.
    throw new Error(
      `로그인 응답에 access_token 이 없습니다. 서버가 redirect_uri 를 거부했을 ` +
        `수 있습니다 (OAUTH_ALLOWED_REDIRECT_URIS 에 ${redirectUri} 추가 필요). ` +
        `받은 redirect: ${redirectUrl}`,
    );
  }
  await setTokens(access, refresh);
  return { access, refresh };
}

/** Pull `access_token` (and optional `refresh_token`) out of a URL
 * fragment. Exported for unit testing. */
export function parseTokensFromUrl(redirectUrl: string): {
  access: string | null;
  refresh: string | null;
} {
  const hashIndex = redirectUrl.indexOf("#");
  if (hashIndex < 0) return { access: null, refresh: null };
  const fragment = redirectUrl.slice(hashIndex + 1);
  const params = new URLSearchParams(fragment);
  return {
    access: params.get("access_token"),
    refresh: params.get("refresh_token"),
  };
}
