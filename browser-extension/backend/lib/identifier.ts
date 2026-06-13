export const Identifier = {
  INPAGE: "dambi-inpage",
  CONTENT_SCRIPT: "dambi-contentscript",
  CONFIRM: "dambi-confirm",
  // Dedicated channel for the MAIN-world fetch hook ↔ its ISOLATED bridge.
  // MUST be distinct from INPAGE/CONTENT_SCRIPT: reusing the provider proxy's
  // channel makes the two `WindowPostMessageStream` handshakes race over the
  // single bridge and cork the stream (see `inject-scripts.ts`).
  FETCH_INPAGE: "dambi-fetch-inpage",
  FETCH_CONTENT_SCRIPT: "dambi-fetch-contentscript",
  // C1: init markers under which the ISOLATED bridges transfer their verdict
  // WRITER-port's reader to the MAIN world. The verdict (the security-critical
  // leg) travels over that MessageChannel, NOT the page-observable stream, so a
  // page cannot forge it. Distinct keys per channel (provider vs fetch).
  VERDICT_PORT_INIT: "dambi-verdict-port-init",
  FETCH_VERDICT_PORT_INIT: "dambi-fetch-verdict-port-init",
  METAMASK_PROVIDER: "metamask-provider",
  METAMASK_INPAGE: "metamask-inpage",
  METAMASK_CONTENT_SCRIPT: "metamask-contentscript",
  COINBASE_WALLET_REQUEST: "extensionUIRequest",
} as const;

export const PROVIDER_MARKER = "__isDambi__" as const;
