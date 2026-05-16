use crate::parse::AdapterArgs;
use proc_macro2::TokenStream;
#[allow(unused_imports)]
use quote::{format_ident, quote};
use serde_json::{json, Value};

pub fn expand(args: AdapterArgs, item: TokenStream) -> TokenStream {
    let ty_ident = match extract_target_ident(&item) {
        Ok(id) => id,
        Err(e) => return e.to_compile_error(),
    };

    let manifest_json = build_manifest_json(&args);
    let manifest_bytes = manifest_json.as_bytes();
    let manifest_len = manifest_bytes.len();
    let manifest_lit = proc_macro2::Literal::byte_string(manifest_bytes);

    let mut exports = Vec::new();
    let mut asserts = Vec::new();

    for cap in &args.capabilities {
        match cap.as_str() {
            "decoder" => {
                asserts.push(quote! {
                    const _: fn() = || {
                        fn _require_decoder<T: ::adapter_sdk::traits::Decoder>() {}
                        _require_decoder::<#ty_ident>();
                    };
                });
                exports.push(decoder_export(&ty_ident));
            }
            "call_adapter" => {
                asserts.push(quote! {
                    const _: fn() = || {
                        fn _require_call_adapter<T: ::adapter_sdk::traits::CallAdapter>() {}
                        _require_call_adapter::<#ty_ident>();
                    };
                });
                exports.push(call_adapter_export(&ty_ident));
            }
            "sign_adapter" => {
                asserts.push(quote! {
                    const _: fn() = || {
                        fn _require_sign_adapter<T: ::adapter_sdk::traits::SignAdapter>() {}
                        _require_sign_adapter::<#ty_ident>();
                    };
                });
                exports.push(sign_adapter_export(&ty_ident));
            }
            other => {
                return syn::Error::new(
                    proc_macro2::Span::call_site(),
                    format!("unknown capability `{other}`"),
                )
                .to_compile_error();
            }
        }
    }

    let manifest_export = quote! {
        #[cfg(target_arch = "wasm32")]
        #[link_section = "adapter_manifest"]
        #[used]
        static __ADAPTER_MANIFEST: [u8; #manifest_len] = *#manifest_lit;

        #[cfg(not(target_arch = "wasm32"))]
        pub const __ADAPTER_MANIFEST_JSON: &str = #manifest_json;

        #[no_mangle]
        #[cfg_attr(target_arch = "wasm32", export_name = "manifest_json")]
        pub extern "C" fn __adapter_manifest_json() -> i64 {
            ::adapter_sdk::abi::pack_result(#manifest_json.as_bytes().to_vec())
        }
    };

    quote! {
        #item
        #(#asserts)*
        #manifest_export
        #(#exports)*
    }
}

fn extract_target_ident(item: &TokenStream) -> syn::Result<proc_macro2::Ident> {
    let parsed: syn::Item = syn::parse2(item.clone())?;
    match parsed {
        syn::Item::Struct(s) => Ok(s.ident),
        syn::Item::Enum(e) => Ok(e.ident),
        other => Err(syn::Error::new_spanned(
            other,
            "#[adapter] must annotate a struct or enum",
        )),
    }
}

fn build_manifest_json(args: &AdapterArgs) -> String {
    // Hardcoded `1` because the proc-macro crate cannot depend on adapter-sdk
    // at expansion time (cyclic) — keep this in sync with `SDK_VERSION` in
    // `crates/adapter-sdk/src/manifest.rs`.
    let mut m = json!({
        "name": args.name,
        "version": args.version,
        "sdk_version": 1u32,
        "description": args.description,
        "capabilities": args.capabilities,
        "applies_to":  args.applies_to.iter().map(|(c, a)| json!({"chain": c, "address": a.to_lowercase()})).collect::<Vec<_>>(),
        "factory_of":  args.factory_of.iter().map(|(c, a)| json!({"chain": c, "factory": a.to_lowercase()})).collect::<Vec<_>>(),
        "proxy_of":    args.proxy_of.iter().map(|(c, a)| json!({"chain": c, "implementation": a.to_lowercase()})).collect::<Vec<_>>(),
    });
    if let Some(a) = &args.author { m.as_object_mut().unwrap().insert("author".into(), Value::String(a.clone())); }
    if let Some(h) = &args.homepage { m.as_object_mut().unwrap().insert("homepage".into(), Value::String(h.clone())); }
    serde_json::to_string(&m).expect("manifest serialization")
}

fn decoder_export(ty: &proc_macro2::Ident) -> TokenStream {
    quote! {
        #[no_mangle]
        pub extern "C" fn decode_call(
            ctx_ptr: *const u8,
            ctx_len: usize,
            calldata_ptr: *const u8,
            calldata_len: usize,
        ) -> i64 {
            ::adapter_sdk::abi::decode_call_entry::<#ty>(
                ctx_ptr, ctx_len, calldata_ptr, calldata_len,
            )
        }
    }
}

fn call_adapter_export(ty: &proc_macro2::Ident) -> TokenStream {
    quote! {
        #[no_mangle]
        pub extern "C" fn map_to_action(
            ctx_ptr: *const u8,
            ctx_len: usize,
            decoded_ptr: *const u8,
            decoded_len: usize,
        ) -> i64 {
            ::adapter_sdk::abi::map_to_action_entry::<#ty>(
                ctx_ptr, ctx_len, decoded_ptr, decoded_len,
            )
        }
    }
}

fn sign_adapter_export(ty: &proc_macro2::Ident) -> TokenStream {
    quote! {
        #[no_mangle]
        pub extern "C" fn decode_sign(
            ctx_ptr: *const u8,
            ctx_len: usize,
            req_ptr: *const u8,
            req_len: usize,
        ) -> i64 {
            ::adapter_sdk::abi::decode_sign_entry::<#ty>(
                ctx_ptr, ctx_len, req_ptr, req_len,
            )
        }
    }
}
