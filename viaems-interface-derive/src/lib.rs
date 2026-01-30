extern crate proc_macro;

use proc_macro::TokenStream;
use quote::{quote, format_ident};
use syn::{self, parse_macro_input, DeriveInput};

// Generate a list of LoggableField structs from a prost-generated message

#[proc_macro_derive(LoggableStruct)]
pub fn make_loggable(input: TokenStream) -> TokenStream { 
    let input = parse_macro_input!(input as DeriveInput);
    let struct_name = input.ident;
    
    let fields = if let syn::Data::Struct(s) = &input.data &&
                    let syn::Fields::Named(f) = &s.fields {
                        &f.named
    } else {
        panic!("Only structs with named fields are supported")
    };

    let loggablefields = fields.into_iter()
        .map(|f| {
            let field_name_str = f.ident.as_ref().unwrap().to_string();
            let ft = &f.ty;
            quote! {
                {
                    let path = prefix.clone() + #field_name_str;
                    result.append(&mut <#ft as crate::interface::LoggableMessage>::get_loggable_fields(&path)); 
                }
            }
    });

    let valuefields = fields.into_iter()
        .map(|f| {
            let field_name = f.ident.as_ref().unwrap();
            let ft = &f.ty;
            quote! {
                result.append(&mut <#ft as crate::interface::LoggableMessage>::get_duckdb_value_list(&self.#field_name)); 
            }
    });

    let expanded = quote! {
        impl crate::interface::LoggableMessage for #struct_name {
            fn get_loggable_fields(prefix: &str) -> Vec<crate::interface::LoggableField> {
                let mut result = vec![];
                let prefix = if prefix == "" {
                    "".to_string()
                } else {
                    format!("{}.", prefix)
                };

                #(#loggablefields) *
                result
            }

            fn get_duckdb_value_list(&self) -> Vec<duckdb::types::Value> {
                let mut result = vec![];
                #(#valuefields) *
                result
            }

        }
    };
    proc_macro::TokenStream::from(expanded)
}

