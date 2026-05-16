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
