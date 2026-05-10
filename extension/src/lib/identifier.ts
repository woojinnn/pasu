export const Identifier = {
  INPAGE: "scopeball-inpage",
  CONTENT_SCRIPT: "scopeball-contentscript",
  CONFIRM: "scopeball-confirm",
  METAMASK_PROVIDER: "metamask-provider",
  METAMASK_INPAGE: "metamask-inpage",
  METAMASK_CONTENT_SCRIPT: "metamask-contentscript",
  COINBASE_WALLET_REQUEST: "extensionUIRequest",
} as const;

export const PROVIDER_MARKER = "__isScopeball__" as const;
