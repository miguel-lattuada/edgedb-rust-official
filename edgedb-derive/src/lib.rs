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
        .map(|f| f.ident.clone().unwrap()).collect::<Vec<_>>();
    let fieldtype = fields.named.iter()
        .map(|f| f.ty.clone()).collect::<Vec<_>>();
    let fieldstr = fieldname.iter()
        .map(|s| syn::LitStr::new(&s.to_string(), s.span()));
    let has_id = fieldname.iter().find(|x| x.to_string() == "id").is_some();
    let has_type_id = fieldname.iter().find(|x| x.to_string() == "__tid__").is_some();
    let implicit_fields =
        if has_id { 0 } else { 1 } +
        if has_type_id { 0 } else { 1 };
    let nfields = fields.named.len()+implicit_fields;
    let fieldno = implicit_fields..nfields;
    let typeid_block = if has_type_id {
        None
    } else {
        Some(quote! {
            ::snafu::ensure!(
                ::bytes::buf::Buf::remaining(buf) >= 8,
                ::edgedb_protocol::errors::Underflow);
            let _reserved = ::bytes::buf::Buf::get_i32(buf);
            let len = ::bytes::buf::Buf::get_u32(buf) as usize;
            ::snafu::ensure!(
                ::bytes::buf::Buf::remaining(buf) >= len,
                ::edgedb_protocol::errors::Underflow);
            ::bytes::buf::Buf::advance(buf, len);
        })
    };
    let id_block = if has_id {
        None
    } else {
        Some(quote! {
            ::snafu::ensure!(
                ::bytes::buf::Buf::remaining(buf) >= 8,
                ::edgedb_protocol::errors::Underflow);
            let _reserved = ::bytes::buf::Buf::get_i32(buf);
            let len = ::bytes::buf::Buf::get_u32(buf) as usize;
            ::snafu::ensure!(
                ::bytes::buf::Buf::remaining(buf) >= len,
                ::edgedb_protocol::errors::Underflow);
            ::bytes::buf::Buf::advance(buf, len);
        })
    };
    let type_id_check = if has_type_id {
        None
    } else {
        Some(quote! {
            if(!shape.elements[0].flag_implicit) {
                return Err(ctx.expected("implicit __tid__"));
            }
        })
    };
    let id_check = if has_id {
        None
    } else {
        let n: usize = if has_type_id { 1 } else { 0 };
        Some(quote! {
            if(!shape.elements[#n].flag_implicit) {
                return Err(ctx.expected("implicit id"));
            }
        })
    };
    let expanded = quote! {
        impl #impl_generics ::edgedb_protocol::queryable::Queryable
            for #name #ty_generics {
            fn decode_raw(buf: &mut ::std::io::Cursor<::bytes::Bytes>)
                -> Result<Self, ::edgedb_protocol::errors::DecodeError>
            {
                ::snafu::ensure!(
                    ::bytes::buf::Buf::remaining(buf) >= 4,
                    ::edgedb_protocol::errors::Underflow);
                let size = ::bytes::buf::Buf::get_u32(buf) as usize;
                ::snafu::ensure!(size == #nfields,
                    ::edgedb_protocol::errors::ObjectSizeMismatch);

                #typeid_block
                #id_block

                #(
                    ::snafu::ensure!(
                        ::bytes::buf::Buf::remaining(buf) >= 8,
                        ::edgedb_protocol::errors::Underflow);
                    let _reserved = ::bytes::buf::Buf::get_i32(buf);
                    let len = ::bytes::buf::Buf::get_u32(buf) as usize;
                    ::snafu::ensure!(
                        ::bytes::buf::Buf::remaining(buf) >= len,
                        ::edgedb_protocol::errors::Underflow);
                    let off = ::std::io::Cursor::position(buf) as usize;
                    let mut chunk = ::std::io::Cursor::new(
                        buf.get_ref().slice(off..off + len));
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
            fn check_descriptor(
                ctx: &::edgedb_protocol::queryable::DescriptorContext,
                type_pos: ::edgedb_protocol::descriptors::TypePos)
                -> Result<(), ::edgedb_protocol::queryable::DescriptorMismatch>
            {
                use ::edgedb_protocol::descriptors::Descriptor::ObjectShape;
                let desc = ctx.get(type_pos)?;
                let shape = match desc {
                    ObjectShape(shape) => shape,
                    _ => {
                        return Err(ctx.wrong_type(desc, "str"))
                    }
                };

                // TODO(tailhook) cache shape.id somewhere

                if(shape.elements.len() != #nfields) {
                    return Err(ctx.field_number(
                        shape.elements.len(), #nfields));
                }
                #type_id_check
                #id_check
                #(
                    let el = &shape.elements[#fieldno];
                    if(el.name != #fieldstr) {
                        return Err(ctx.wrong_field(&el.name, #fieldstr));
                    }
                    <#fieldtype as ::edgedb_protocol::queryable::Queryable>
                        ::check_descriptor(ctx, el.type_pos)?;
                )*
                Ok(())
            }
        }
    };

    // Hand the output tokens back to the compiler
    TokenStream::from(expanded)
}
