use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Fields};

#[proc_macro_derive(ToFields)]
pub fn to_fields_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let fields = match &input.data {
        Data::Struct(data_struct) => match &data_struct.fields {
            Fields::Named(fields) => &fields.named,
            _ => panic!("ToFields can only be derived for structs with named fields"),
        },
        _ => panic!("ToFields can only be derived for structs"),
    };

    let insertions = fields.iter().map(|f| {
        let field_name = &f.ident;
        let field_name_str = field_name.as_ref().unwrap().to_string();

        // Check for #[serde(flatten)] attribute
        let is_flatten = f.attrs.iter().any(|attr| if let syn::Meta::List(meta) = &attr.meta {
            meta.path.is_ident("serde") && meta.tokens.to_string().contains("flatten")
        } else {
            false
        });

        if is_flatten {
            quote! {
                fields.extend(self.#field_name.to_fields());
            }
        } else {
            quote! {
                if let Some(val) = lc_core::traits::IntoFieldString::into_field_string(&self.#field_name) {
                    fields.insert(#field_name_str.to_string(), val);
                }
            }
        }
    });

    let expanded = quote! {
        impl lc_core::traits::ToFields for #name {
            fn to_fields(&self) -> std::collections::HashMap<String, String> {
                let mut fields = std::collections::HashMap::new();
                #(#insertions)*
                fields
            }
        }
    };

    TokenStream::from(expanded)
}

#[proc_macro_derive(MorphologyInfo)]
pub fn morphology_info_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let variants = match &input.data {
        Data::Enum(data_enum) => &data_enum.variants,
        _ => panic!("MorphologyInfo can only be derived for enums"),
    };

    // Verify every variant has a `lemma` field
    for variant in variants {
        let has_lemma = match &variant.fields {
            Fields::Named(fields) => fields
                .named
                .iter()
                .any(|f| f.ident.as_ref().is_some_and(|id| id == "lemma")),
            _ => false,
        };
        if !has_lemma {
            panic!(
                "MorphologyInfo: variant `{}` must have a named `lemma` field",
                variant.ident
            );
        }
    }

    let lemma_arms = variants.iter().map(|v| {
        let ident = &v.ident;
        quote! { Self::#ident { lemma, .. } => lemma, }
    });

    let pos_label_arms = variants.iter().map(|v| {
        let ident = &v.ident;
        let label = ident.to_string();
        quote! { Self::#ident { .. } => #label, }
    });

    let expanded = quote! {
        impl lc_core::traits::MorphologyInfo for #name {
            fn lemma(&self) -> &str {
                match self {
                    #(#lemma_arms)*
                }
            }

            fn pos_label(&self) -> &'static str {
                match self {
                    #(#pos_label_arms)*
                }
            }
        }
    };

    TokenStream::from(expanded)
}
