export const Identifier = {
  INPAGE: "scopeball-inpage",
  CONTENT_SCRIPT: "scopeball-contentscript",
  CONFIRM: "scopeball-confirm",
  // Dedicated channel for the MAIN-world fetch hook ↔ its ISOLATED bridge.
  // MUST be distinct from INPAGE/CONTENT_SCRIPT: reusing the provider proxy's
  // channel makes the two `WindowPostMessageStream` handshakes race over the
  // single bridge and cork the stream (see `inject-scripts.ts`).
  FETCH_INPAGE: "scopeball-fetch-inpage",
  FETCH_CONTENT_SCRIPT: "scopeball-fetch-contentscript",
  METAMASK_PROVIDER: "metamask-provider",
  METAMASK_INPAGE: "metamask-inpage",
  METAMASK_CONTENT_SCRIPT: "metamask-contentscript",
  COINBASE_WALLET_REQUEST: "extensionUIRequest",
} as const;

export const PROVIDER_MARKER = "__isScopeball__" as const;
