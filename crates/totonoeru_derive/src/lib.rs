extern crate proc_macro2;

use proc_macro::TokenStream;

#[proc_macro_derive(Config)]
pub fn derive_config(_item: TokenStream) -> TokenStream {
    todo!()
}