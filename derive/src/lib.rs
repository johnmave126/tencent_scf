//! The derive helper for [`tencent_scf`]
//!
//! # Derives
//! * `AsJson`: Mark a type as `AsJson`, no method provided.
//!
//! # Acknowledgement
//! The code is adapted from the `heapsize` example from `syn` crate.
//!
//! [`tencent_scf`]: https://crates.io/crates/tencent_scf

use quote::quote;
use syn::{parse_macro_input, DeriveInput};

#[proc_macro_derive(AsJson)]
pub fn derive_as_json(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    // Parse the input
    let input = parse_macro_input!(input as DeriveInput);

    // Name of the type
    let name = input.ident;

    // Extract generic parameters
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let expanded = quote! {
        // The generated impl.
        impl #impl_generics tencent_scf::convert::AsJson for #name #ty_generics #where_clause {}
    };

    // Hand the output tokens back to the compiler
    proc_macro::TokenStream::from(expanded)
}
