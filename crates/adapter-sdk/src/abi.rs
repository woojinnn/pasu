//! ABI plumbing exposed to macro-generated code.
//! Adapter authors should NOT call these directly.

#![allow(unsafe_code)]

use std::alloc::{alloc as sys_alloc, dealloc as sys_dealloc, Layout};

/// Allocate `size` bytes inside the adapter's linear memory.
/// Caller responsibility: pair every `alloc` with `dealloc` of the same size.
/// Exported to WASM as the bare name `alloc` (spec §6); on non-wasm targets
/// this is a regular Rust function (callable in host-side tests but not
/// exposed as a public symbol).
#[doc(hidden)]
#[cfg_attr(target_arch = "wasm32", export_name = "alloc")]
pub extern "C" fn adapter_alloc(size: usize) -> *mut u8 {
    if size == 0 {
        return std::ptr::null_mut();
    }
    let layout = Layout::from_size_align(size, 1).expect("layout");
    unsafe { sys_alloc(layout) }
}

#[doc(hidden)]
#[cfg_attr(target_arch = "wasm32", export_name = "dealloc")]
pub extern "C" fn adapter_dealloc(ptr: *mut u8, size: usize) {
    if ptr.is_null() || size == 0 {
        return;
    }
    let layout = Layout::from_size_align(size, 1).expect("layout");
    unsafe { sys_dealloc(ptr, layout) };
}

/// Pack a `(ptr, len)` pair into a single i64 (high = ptr, low = len).
/// WASM exports return i64 because returning two i32s requires the
/// multi-value proposal which is not yet ubiquitous.
///
/// On wasm32, pointers fit in u32 so the upper half is the pointer.
/// On non-wasm targets, native pointers are 64-bit and would truncate —
/// this function is therefore gated to wasm32; non-wasm callers (host tests)
/// must not invoke it.
#[doc(hidden)]
#[cfg(target_arch = "wasm32")]
pub fn pack_result(bytes: Vec<u8>) -> i64 {
    let len = bytes.len() as u32;
    let boxed = bytes.into_boxed_slice();
    let ptr = Box::into_raw(boxed) as *mut u8 as u32;
    ((ptr as i64) << 32) | (len as i64)
}

/// Non-wasm host-test stub. Returns 0 so any leaked call surface a quick
/// `NotFound`-style result rather than UB.
#[doc(hidden)]
#[cfg(not(target_arch = "wasm32"))]
pub fn pack_result(_bytes: Vec<u8>) -> i64 {
    0
}

/// SAFETY: caller must guarantee the pointer was produced by allocating
/// `size` bytes via `adapter_alloc` (or the wasm-exported `alloc`) and is
/// initialized over its full length.
#[doc(hidden)]
pub unsafe fn read_input(ptr: *const u8, len: usize) -> Vec<u8> {
    if ptr.is_null() || len == 0 {
        return Vec::new();
    }
    let slice = std::slice::from_raw_parts(ptr, len);
    slice.to_vec()
}

use crate::action::ActionEnvelope;
use crate::ctx::{CallCtx, SignCtx};
use crate::error::{AdapterError, CtxError, LogLevel};
use crate::primitives::{Address, ChainId, Selector};
use crate::sign::SignRequest;
use crate::traits::{CallAdapter, Decoder, SignAdapter};
use crate::types::DecodedCall;
use serde::{Deserialize, Serialize};

// Host imports (wasm32 only). The JS loader provides these when instantiating the WASM.
// `link_name` keeps the import symbols exactly `log` / `lookup_adapter` so the
// JS host's `env.log` / `env.lookup_adapter` bindings resolve.
#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "env")]
extern "C" {
    #[link_name = "log"]
    fn host_log(level: i32, msg_ptr: *const u8, msg_len: usize);
    #[link_name = "lookup_adapter"]
    fn host_lookup_adapter(
        chain: u64,
        addr_ptr: *const u8,
        calldata_ptr: *const u8,
        calldata_len: usize,
    ) -> i64;
}

// Non-WASM stubs. PRIVATE functions (no #[no_mangle]) so they don't collide
// with libm's `log` or other crates' symbols at link time. They share the
// `host_log` / `host_lookup_adapter` names so the call sites below compile
// uniformly across targets.
#[cfg(not(target_arch = "wasm32"))]
unsafe fn host_log(_level: i32, _msg_ptr: *const u8, _msg_len: usize) {}

#[cfg(not(target_arch = "wasm32"))]
unsafe fn host_lookup_adapter(
    _chain: u64,
    _addr_ptr: *const u8,
    _calldata_ptr: *const u8,
    _calldata_len: usize,
) -> i64 {
    // (null, 0) — caller interprets as CtxError::NotFound.
    0
}

/// JSON wire form of the call context (without function-pointer callbacks).
#[derive(Debug, Clone, Deserialize, Serialize)]
struct CallCtxWire {
    chain_id: ChainId,
    target: Address,
    selector: Selector,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct SignCtxWire {
    chain_id: ChainId,
    verifying_contract: Address,
    primary_type: String,
}

fn log_via_host(level: LogLevel, msg: &str) {
    unsafe {
        host_log(level as i32, msg.as_ptr(), msg.len());
    }
}

fn lookup_adapter_via_host(
    chain: ChainId,
    addr: Address,
    calldata: &[u8],
) -> Result<DecodedCall, CtxError> {
    let packed = unsafe {
        host_lookup_adapter(chain, addr.0.as_ptr(), calldata.as_ptr(), calldata.len())
    };
    if packed == 0 {
        return Err(CtxError::NotFound { chain, address: addr.to_string() });
    }
    let ptr = (packed >> 32) as *mut u8;
    let len = (packed & 0xFFFF_FFFF) as usize;
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) }.to_vec();
    // The host allocated this buffer via `alloc`; free it after copy.
    adapter_dealloc(ptr, len);
    serde_json::from_slice::<Result<DecodedCall, CtxError>>(&bytes)
        .map_err(|e| CtxError::Host { message: format!("host returned malformed json: {e}") })?
}

#[doc(hidden)]
pub fn decode_call_entry<T: Decoder + Default>(
    ctx_ptr: *const u8,
    ctx_len: usize,
    calldata_ptr: *const u8,
    calldata_len: usize,
) -> i64 {
    let ctx_bytes = unsafe { read_input(ctx_ptr, ctx_len) };
    let calldata = unsafe { read_input(calldata_ptr, calldata_len) };

    let wire: CallCtxWire = match serde_json::from_slice(&ctx_bytes) {
        Ok(w) => w,
        Err(e) => {
            return pack_err::<DecodedCall>(AdapterError::DecodeFailed {
                message: format!("ctx parse: {e}"),
            })
        }
    };
    let log_closure: &dyn Fn(LogLevel, &str) = &|lvl, msg| log_via_host(lvl, msg);
    let lookup_closure: &dyn Fn(ChainId, Address, &[u8]) -> Result<DecodedCall, CtxError> =
        &|chain, addr, data| lookup_adapter_via_host(chain, addr, data);
    let ctx = CallCtx {
        chain_id: wire.chain_id,
        target: wire.target,
        selector: wire.selector,
        log: log_closure,
        lookup_adapter: lookup_closure,
    };

    let adapter = T::default();
    let result = adapter.decode_call(&ctx, &calldata);
    pack_json(&result)
}

#[doc(hidden)]
pub fn map_to_action_entry<T: CallAdapter + Default>(
    ctx_ptr: *const u8,
    ctx_len: usize,
    decoded_ptr: *const u8,
    decoded_len: usize,
) -> i64 {
    let ctx_bytes = unsafe { read_input(ctx_ptr, ctx_len) };
    let decoded_bytes = unsafe { read_input(decoded_ptr, decoded_len) };

    let wire: CallCtxWire = match serde_json::from_slice(&ctx_bytes) {
        Ok(w) => w,
        Err(e) => {
            return pack_err::<Vec<ActionEnvelope>>(AdapterError::DecodeFailed {
                message: format!("ctx parse: {e}"),
            })
        }
    };
    let decoded: DecodedCall = match serde_json::from_slice(&decoded_bytes) {
        Ok(d) => d,
        Err(e) => {
            return pack_err::<Vec<ActionEnvelope>>(AdapterError::DecodeFailed {
                message: format!("decoded parse: {e}"),
            })
        }
    };
    let log_closure: &dyn Fn(LogLevel, &str) = &|lvl, msg| log_via_host(lvl, msg);
    let lookup_closure: &dyn Fn(ChainId, Address, &[u8]) -> Result<DecodedCall, CtxError> =
        &|chain, addr, data| lookup_adapter_via_host(chain, addr, data);
    let ctx = CallCtx {
        chain_id: wire.chain_id,
        target: wire.target,
        selector: wire.selector,
        log: log_closure,
        lookup_adapter: lookup_closure,
    };

    let adapter = T::default();
    let result = adapter.map_to_action(&ctx, &decoded);
    pack_json(&result)
}

#[doc(hidden)]
pub fn decode_sign_entry<T: SignAdapter + Default>(
    ctx_ptr: *const u8,
    ctx_len: usize,
    req_ptr: *const u8,
    req_len: usize,
) -> i64 {
    let ctx_bytes = unsafe { read_input(ctx_ptr, ctx_len) };
    let req_bytes = unsafe { read_input(req_ptr, req_len) };

    let wire: SignCtxWire = match serde_json::from_slice(&ctx_bytes) {
        Ok(w) => w,
        Err(e) => {
            return pack_err::<Vec<ActionEnvelope>>(AdapterError::DecodeFailed {
                message: format!("ctx parse: {e}"),
            })
        }
    };
    let req: SignRequest = match serde_json::from_slice(&req_bytes) {
        Ok(r) => r,
        Err(e) => {
            return pack_err::<Vec<ActionEnvelope>>(AdapterError::DecodeFailed {
                message: format!("sign request parse: {e}"),
            })
        }
    };
    let log_closure: &dyn Fn(LogLevel, &str) = &|lvl, msg| log_via_host(lvl, msg);
    let lookup_closure: &dyn Fn(ChainId, Address, &[u8]) -> Result<DecodedCall, CtxError> =
        &|chain, addr, data| lookup_adapter_via_host(chain, addr, data);
    let ctx = SignCtx {
        chain_id: wire.chain_id,
        verifying_contract: wire.verifying_contract,
        primary_type: wire.primary_type,
        log: log_closure,
        lookup_adapter: lookup_closure,
    };

    let adapter = T::default();
    let result = adapter.decode_sign(&ctx, &req);
    pack_json(&result)
}

fn pack_json<T: Serialize>(r: &Result<T, AdapterError>) -> i64 {
    let bytes = serde_json::to_vec(r).expect("serialize adapter result");
    pack_result(bytes)
}

fn pack_err<T: Serialize>(err: AdapterError) -> i64 {
    let r: Result<T, AdapterError> = Err(err);
    let bytes = serde_json::to_vec(&r).expect("serialize adapter error");
    pack_result(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alloc_dealloc_roundtrip() {
        let p = adapter_alloc(64);
        assert!(!p.is_null());
        adapter_dealloc(p, 64);
    }

    // pack_result deliberately untested on host: it's wasm32-only because
    // host pointers don't fit in u32 (UB on truncation).
}
