import Browser from "webextension-polyfill";

function injectScript(url: string): void {
  const container = document.head ?? document.documentElement;
  const script = document.createElement("script");
  script.setAttribute("async", "false");
  script.setAttribute("src", Browser.runtime.getURL(url));
  container.appendChild(script);
  script.onload = () => script.remove();
}

// On Chrome MV3 the proxy is injected directly into MAIN world via the
// dedicated `content_scripts` entry with `world: "MAIN"` in the manifest.
// Running this legacy <script>-tag injection on top of that would create
// a second copy of proxy-injected-providers.js inside the same page, and
// the two copies would race to handshake over `WindowPostMessageStream`
// with the (single) bridge in ISOLATED world — leaving at least one of
// the streams perpetually corked, so wallet messages never reach the
// service worker. Firefox doesn't support content_scripts.world yet, so
// keep the script-tag fallback for MV2 builds.
if (Browser.runtime.getManifest().manifest_version !== 3) {
  injectScript("js/injected/proxy-injected-providers.js");
}
