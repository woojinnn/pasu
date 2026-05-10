import type { Address, Hex } from "viem";

export enum RequestType {
  TRANSACTION = "transaction",
  TYPED_SIGNATURE = "typed-signature",
  UNTYPED_SIGNATURE = "untyped-signature",
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

export interface TransactionHashReportPayload {
  type: "tx-hash-report";
  requestId: string;
  txHash: Hex;
  hostname: string;
}

export type MessageData =
  | TransactionPayload
  | TypedSignaturePayload
  | UntypedSignaturePayload
  | RawTransactionAdvisoryPayload
  | FrozenProviderWarningPayload
  | TransactionHashReportPayload;

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
