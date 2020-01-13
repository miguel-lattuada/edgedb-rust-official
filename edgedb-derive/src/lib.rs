extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::{self, parse_macro_input};


#[proc_macro_derive(Queryable, attributes(edgedb))]
pub fn edgedb_queryable(input: TokenStream) -> TokenStream {
    let s = parse_macro_input!(input as syn::ItemStruct);

    let name = s.ident;
    let (impl_generics, ty_generics, _) = s.generics.split_for_impl();
    let fields = match s.fields {
        syn::Fields::Named(named) => named,
        _ => {
            return syn::Error::new_spanned(
                s.fields, "only named fields are supported")
                .to_compile_error()
                .into();
        }
    };
    let fieldname = fields.named.iter()
        .map( | f | f.ident.clone().unwrap()).collect::<Vec<_>>();
    let nfields = fields.named.len();
    let expanded = quote! {
        impl #impl_generics ::edgedb_protocol::queryable::Queryable
            for #name #ty_generics {
            fn decode_raw(buf: &mut ::std::io::Cursor<::bytes::Bytes>)
                -> Result<Self, ::edgedb_protocol::errors::DecodeError>
            {
                ::snafu::ensure!(
                    ::bytes::buf::Buf::remaining(buf) >= 4,
                    ::edgedb_protocol::errors::Underflow);
                let size = ::bytes::buf::Buf::get_u32_be(buf) as usize;
                ::snafu::ensure!(size == #nfields + 2,
                    ::edgedb_protocol::errors::ObjectSizeMismatch);

                // Skip typeid
                // TODO(tailhook) don't skip if it's in the structure
                ::snafu::ensure!(
                    ::bytes::buf::Buf::remaining(buf) >= 8,
                    ::edgedb_protocol::errors::Underflow);
                let _reserved = ::bytes::buf::Buf::get_i32_be(buf);
                let len = ::bytes::buf::Buf::get_u32_be(buf) as usize;
                ::snafu::ensure!(
                    ::bytes::buf::Buf::remaining(buf) >= len,
                    ::edgedb_protocol::errors::Underflow);
                ::bytes::buf::Buf::advance(buf, len);

                // Skip id
                // TODO(tailhook) don't skip if it's in the structure
                ::snafu::ensure!(
                    ::bytes::buf::Buf::remaining(buf) >= 8,
                    ::edgedb_protocol::errors::Underflow);
                let _reserved = ::bytes::buf::Buf::get_i32_be(buf);
                let len = ::bytes::buf::Buf::get_u32_be(buf) as usize;
                ::snafu::ensure!(
                    ::bytes::buf::Buf::remaining(buf) >= len,
                    ::edgedb_protocol::errors::Underflow);
                ::bytes::buf::Buf::advance(buf, len);

                #(
                    ::snafu::ensure!(
                        ::bytes::buf::Buf::remaining(buf) >= 8,
                        ::edgedb_protocol::errors::Underflow);
                    let _reserved = ::bytes::buf::Buf::get_i32_be(buf);
                    let len = ::bytes::buf::Buf::get_u32_be(buf) as usize;
                    ::snafu::ensure!(
                        ::bytes::buf::Buf::remaining(buf) >= len,
                        ::edgedb_protocol::errors::Underflow);
                    let off = ::std::io::Cursor::position(buf) as usize;
                    let mut chunk = ::std::io::Cursor::new(
                        buf.get_ref().slice(off, off + len));
                    ::bytes::buf::Buf::advance(buf, len);
                    let #fieldname =
                        ::edgedb_protocol::queryable::Queryable::decode(
                            &mut chunk)?;
                )*
                Ok(#name {
                    #(
                        #fieldname,
                    )*
                })
            }
        }
    };

    // Hand the output tokens back to the compiler
    TokenStream::from(expanded)
}
