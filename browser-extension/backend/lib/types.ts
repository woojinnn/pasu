import type { Address, Hex } from "viem";

export enum RequestType {
  TRANSACTION = "transaction",
  TYPED_SIGNATURE = "typed-signature",
  UNTYPED_SIGNATURE = "untyped-signature",
  // Off-chain venue order intercepted from a network POST (e.g. Hyperliquid
  // `/exchange`). Unlike the three above, it never flows through
  // `window.ethereum` — the dApp signs with an agent key and POSTs directly —
  // so it is captured by the MAIN-world fetch hook, not the provider proxy.
  VENUE_ORDER = "venue-order",
  EXECUTION_REPORT = "execution-report",
}

export interface TransactionPayload {
  type: RequestType.TRANSACTION;
  chainId: number;
  hostname: string;
  bypassed?: boolean;
  transaction: {
    from?: Address;
    to?: Address;
    data?: Hex;
    value?: string;
  };
}

export interface TypedSignaturePayload {
  type: RequestType.TYPED_SIGNATURE;
  chainId: number;
  hostname: string;
  bypassed?: boolean;
  address: Address;
  typedData: unknown;
}

export interface UntypedSignaturePayload {
  type: RequestType.UNTYPED_SIGNATURE;
  hostname: string;
  bypassed?: boolean;
  message: string;
}

/** One Hyperliquid `/exchange` order-wire entry (`action.orders[i]`). */
export interface HyperliquidOrderWire {
  /** Asset index (perp = meta.universe index; spot = 10000 + spotMeta index). */
  a: number;
  /** isBuy — `true` ⇒ long/buy, `false` ⇒ short/sell. */
  b: boolean;
  /** Limit price, decimal string. */
  p: string;
  /** Size in base units, decimal string. */
  s: string;
  /** reduceOnly. */
  r?: boolean;
  /** Order type — `{ limit: { tif } }` or `{ trigger: {...} }`. */
  t?: unknown;
  /** Optional client order id (128-bit hex). */
  c?: string;
}

/**
 * One parsed Hyperliquid CORE action, discriminated by `kind`. Mirrors the
 * v1 high-risk subset the engine's `ActionBody::HyperliquidCore` variant models
 * (an order leg, a leverage change, and the three fund-movement / delegation
 * actions). The SW converter (`hl-order-to-action.ts`) maps each variant to the
 * matching `ActionBody` JSON.
 */
export type VenueActionWire =
  | { kind: "order"; order: HyperliquidOrderWire }
  | {
      kind: "update_leverage";
      /** Asset index (`asset`). */
      assetIndex: number;
      /** `isCross` — cross vs isolated margin. */
      isCross: boolean;
      /** New leverage multiplier. */
      leverage: number;
    }
  | { kind: "withdraw"; destination: string; amount: string }
  | { kind: "usd_send"; destination: string; amount: string }
  | { kind: "spot_send"; destination: string; token: string; amount: string }
  | { kind: "usd_class_transfer"; amount: string; toPerp: boolean }
  | {
      kind: "send_asset";
      destination: string;
      sourceDex: string;
      destinationDex: string;
      token: string;
      amount: string;
    }
  | {
      kind: "send_to_evm_with_data";
      token: string;
      amount: string;
      sourceDex: string;
      destinationRecipient: string;
      data: string;
    }
  | { kind: "c_deposit"; wei: string }
  | { kind: "c_withdraw"; wei: string }
  | { kind: "vault_transfer"; vaultAddress: string; isDeposit: boolean; usd: string }
  | {
      kind: "sub_account_transfer";
      subAccountUser: string;
      isDeposit: boolean;
      usd: string;
    }
  | { kind: "token_delegate"; validator: string; isUndelegate: boolean; wei: string }
  | {
      kind: "twap_order";
      assetIndex: number;
      isBuy: boolean;
      size: string;
      reduceOnly: boolean;
      minutes: number;
      randomize: boolean;
    }
  | { kind: "update_isolated_margin"; assetIndex: number; isBuy: boolean; ntli: string }
  | {
      /**
       * Catch-all for an `/exchange` action with no explicit model. Carries only
       * the raw wire `type` string so the engine can gate / surface it (maps to
       * `ActionBody::HyperliquidCore(HlUnknown)` — policy default warn / deny).
       */
      kind: "unknown";
      actionType: string;
    };

/**
 * An off-chain venue action intercepted from a network POST. Carries one parsed
 * Hyperliquid CORE action plus, for market actions, the resolved asset `symbol`
 * (the order wire only has the numeric index; the SW resolves it from a `meta`
 * cache).
 */
export interface VenueOrderPayload {
  type: RequestType.VENUE_ORDER;
  /** Settlement/venue chain hint. `0` for pure off-chain venues (Hyperliquid). */
  chainId: number;
  hostname: string;
  bypassed?: boolean;
  /** Venue id, e.g. `"hyperliquid"`. */
  venue: string;
  /** The intercepted endpoint URL (the `/exchange` POST target). */
  endpoint: string;
  /** The parsed CORE action. */
  hlAction: VenueActionWire;
  /** Resolved asset symbol (e.g. `"BTC-USD"`); `undefined` until meta resolves. */
  symbol?: string;
  /**
   * The `/exchange` request nonce — a millisecond wall-clock timestamp. Shared
   * by every leg of one POST. Threaded into `ActionMeta.submitted_at` (÷1000 →
   * seconds) so time-scoped policies see the real submission time instead of a
   * placeholder.
   */
  nonce?: number;
  /**
   * The `vaultAddress` from the request body when the order is placed on behalf
   * of a vault (`null`/absent otherwise). Captured for attribution; not yet
   * surfaced into the policy context (that needs a shared `ActionMeta` field).
   */
  vaultAddress?: string;
  /**
   * Optional wallet attribution. Hyperliquid agent-key exchange requests do not
   * always reveal the master account in the request body, so the response hook
   * may report without this field and let a later venue sync reconcile state.
   */
  wallet_id?: WalletIdWire;
}

export interface WalletIdWire {
  address: string;
  chains: string[];
}

export type ExecutionReportOutcome =
  | { kind: "wallet_rejected"; reason?: string }
  | { kind: "wallet_confirmed"; method: string }
  | { kind: "wallet_signed"; signature: string }
  | { kind: "onchain_submitted"; chain: string; tx_hash: string }
  | {
      kind: "onchain_confirmed";
      chain: string;
      tx_hash: string;
      block_number?: number;
    }
  | {
      kind: "venue_submitted";
      venue: string;
      client_order_id?: string;
    }
  | {
      kind: "venue_accepted";
      venue: string;
      venue_order_id?: string;
      client_order_id?: string;
    }
  | { kind: "venue_rejected"; venue: string; reason: string }
  | { kind: "failed"; reason: string };

export interface ExecutionReportPayload {
  type: RequestType.EXECUTION_REPORT;
  hostname: string;
  bypassed?: boolean;
  wallet_id?: WalletIdWire;
  evaluation_id?: string;
  action_index?: number;
  outcome: ExecutionReportOutcome;
  metadata?: Record<string, unknown>;
}

export interface RawTransactionAdvisoryPayload {
  type: "raw-transaction-advisory";
  hostname: string;
  rawPreview: string;
}

export interface FrozenProviderWarningPayload {
  type: "provider-frozen-warning";
  hostname: string;
  providerName: string;
}

export type MessageData =
  | TransactionPayload
  | TypedSignaturePayload
  | UntypedSignaturePayload
  | VenueOrderPayload
  | ExecutionReportPayload
  | RawTransactionAdvisoryPayload
  | FrozenProviderWarningPayload;

export interface Message {
  requestId: string;
  data: MessageData;
}

export interface MessageResponse {
  requestId: string;
  data: boolean;
}

export interface AwaitingUserMessage {
  requestId: string;
  kind: "awaiting-user";
}

export type StreamResponse = MessageResponse | AwaitingUserMessage;

export const isTransaction = (
  message: Message,
): message is Message & { data: TransactionPayload } =>
  message.data.type === RequestType.TRANSACTION;

export const isTypedSignature = (
  message: Message,
): message is Message & { data: TypedSignaturePayload } =>
  message.data.type === RequestType.TYPED_SIGNATURE;

export const isUntypedSignature = (
  message: Message,
): message is Message & { data: UntypedSignaturePayload } =>
  message.data.type === RequestType.UNTYPED_SIGNATURE;

export const isVenueOrder = (
  message: Message,
): message is Message & { data: VenueOrderPayload } =>
  message.data.type === RequestType.VENUE_ORDER;

export const isExecutionReport = (
  message: Message,
): message is Message & { data: ExecutionReportPayload } =>
  message.data.type === RequestType.EXECUTION_REPORT;
