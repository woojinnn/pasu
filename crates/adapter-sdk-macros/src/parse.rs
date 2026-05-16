//! Parse `#[adapter(...)]` arguments into a structured `AdapterArgs`.
//! Filled in by Task 10.

use proc_macro2::TokenStream;
use syn::Result;

pub struct AdapterArgs {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: Option<String>,
    pub homepage: Option<String>,
    pub capabilities: Vec<String>,
    pub applies_to: Vec<(u64, String)>,
    pub factory_of: Vec<(u64, String)>,
    pub proxy_of: Vec<(u64, String)>,
}

pub fn parse_args(_input: TokenStream) -> Result<AdapterArgs> {
    Err(syn::Error::new(
        proc_macro2::Span::call_site(),
        "#[adapter] argument parsing not implemented yet",
    ))
}
