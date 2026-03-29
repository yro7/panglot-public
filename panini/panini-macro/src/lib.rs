use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Fields};

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

    // Generate the PosTag enum name: <Name>PosTag
    let pos_tag_name = quote::format_ident!("{}PosTag", name);

    let pos_tag_variants: Vec<_> = variants.iter().map(|v| &v.ident).collect();

    let lemma_arms = variants.iter().map(|v| {
        let ident = &v.ident;
        quote! { Self::#ident { lemma, .. } => lemma, }
    });

    let pos_label_arms = variants.iter().map(|v| {
        let ident = &v.ident;
        let label = ident.to_string();
        quote! { Self::#ident { .. } => #label, }
    });

    let pos_tag_arms = variants.iter().map(|v| {
        let ident = &v.ident;
        quote! { Self::#ident { .. } => #pos_tag_name::#ident, }
    });

    let expanded = quote! {
        /// Auto-generated POS tag enum for use in `MorphemeDefinition::applies_to`.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum #pos_tag_name {
            #(#pos_tag_variants,)*
        }

        impl panini_core::traits::MorphologyInfo for #name {
            type PosTag = #pos_tag_name;

            fn lemma(&self) -> &str {
                match self {
                    #(#lemma_arms)*
                }
            }

            fn pos_tag(&self) -> #pos_tag_name {
                match self {
                    #(#pos_tag_arms)*
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
