export const Identifier = {
  INPAGE: "pasu-inpage",
  CONTENT_SCRIPT: "pasu-contentscript",
  CONFIRM: "pasu-confirm",
  // Dedicated channel for the MAIN-world fetch hook ↔ its ISOLATED bridge.
  // MUST be distinct from INPAGE/CONTENT_SCRIPT: reusing the provider proxy's
  // channel makes the two `WindowPostMessageStream` handshakes race over the
  // single bridge and cork the stream (see `inject-scripts.ts`).
  FETCH_INPAGE: "pasu-fetch-inpage",
  FETCH_CONTENT_SCRIPT: "pasu-fetch-contentscript",
  METAMASK_PROVIDER: "metamask-provider",
  METAMASK_INPAGE: "metamask-inpage",
  METAMASK_CONTENT_SCRIPT: "metamask-contentscript",
  COINBASE_WALLET_REQUEST: "extensionUIRequest",
} as const;

export const PROVIDER_MARKER = "__isPasu__" as const;
