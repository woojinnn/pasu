/* tslint:disable */
/* eslint-disable */

/**
 * Install `console.error` panic hook so wasm panics surface in DevTools.
 * Called automatically when the wasm module is first instantiated.
 */
export function _start(): void;

/**
 * `simulate_sequence(steps_json, policies_json) -> JSON SequenceResp`.
 * Mirrors the old `POST /simulate/sequence` route. Each step is
 * evaluated against every supplied policy; per-step verdict is the
 * worst of {pass, warn, fail} across deny outcomes (warn-severity
 * policies count as warn, deny-severity count as fail). Overall is
 * the worst of the per-step verdicts.
 */
export function simulate_sequence(steps_json: string, policies_json: string): string;

/**
 * `test_policy(text, request_json) -> JSON TestResp`. Mirrors the old
 * `POST /policies/:id/test` route — schema-less Authorizer over a
 * single ad-hoc Cedar request.
 *
 * `request_json` must deserialize to `CedarRequestInput` (matching the
 * pre-existing FE shape: `principal`, `action`, `resource` as
 * `Type::"id"` strings, plus optional `entities` and `context`).
 */
export function test_policy(text: string, request_json: string): string;

/**
 * `validate_policy(text) -> JSON ValidateResp`. Mirrors the old
 * `POST /policies/validate` route — parse-only, no schema attached.
 */
export function validate_policy(text: string): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly simulate_sequence: (a: number, b: number, c: number, d: number) => [number, number];
    readonly test_policy: (a: number, b: number, c: number, d: number) => [number, number];
    readonly validate_policy: (a: number, b: number) => [number, number];
    readonly _start: () => void;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
