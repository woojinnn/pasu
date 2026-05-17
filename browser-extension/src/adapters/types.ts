export type ChainId = number;
export type Hex = `0x${string}`;

export interface Manifest {
  name: string;
  version: string;
  sdk_version: number;
  description: string;
  author?: string;
  homepage?: string;
  capabilities: Capability[];
  applies_to: AppliesTo[];
  factory_of: FactoryOf[];
  proxy_of: ProxyOf[];
}

export type Capability = "decoder" | "call_adapter" | "sign_adapter";

export interface AppliesTo { chain: ChainId; address: Hex; }
export interface FactoryOf { chain: ChainId; factory: Hex; }
export interface ProxyOf { chain: ChainId; implementation: Hex; }

export interface DecodedCall {
  chain_id: ChainId;
  target: Hex;
  selector: Hex;
  function: string;
  args: DecodedArg[];
  nested?: DecodedCall[];
}

export interface DecodedArg { name: string; value: DecodedValue; }

export type DecodedValue =
  | { type: "address"; value: Hex }
  | { type: "uint"; value: string }
  | { type: "int"; value: string }
  | { type: "bool"; value: boolean }
  | { type: "bytes"; value: Hex }
  | { type: "string"; value: string }
  | { type: "tuple"; value: DecodedValue[] }
  | { type: "array"; value: DecodedValue[] };

export interface Action {
  kind: "other" | "custom";
  chain_id: ChainId;
  target: Hex;
  decoded?: DecodedCall;
  name?: string;
  fields?: unknown;
}

export interface ActionEnvelope { action: Action; trace?: unknown; }

export type AdapterError =
  | { kind: "calldata_too_short"; expected: number; got: number }
  | { kind: "unknown_selector"; selector: string }
  | { kind: "decode_failed"; message: string }
  | { kind: "invariant"; message: string };

export type CtxError =
  | { kind: "cycle"; chain: number; address: string }
  | { kind: "depth_exceeded" }
  | { kind: "not_found"; chain: number; address: string }
  | { kind: "host"; message: string };

export type AdapterResult<T> = { Ok: T } | { Err: AdapterError };
