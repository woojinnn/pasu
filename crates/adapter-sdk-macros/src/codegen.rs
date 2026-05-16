use proc_macro2::TokenStream;
use crate::parse::AdapterArgs;

pub fn expand(_args: AdapterArgs, item: TokenStream) -> TokenStream {
    item
}
