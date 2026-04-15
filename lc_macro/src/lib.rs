use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Fields};

/// # Panics
///
/// Panics if the input is not a struct with named fields.
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
                if let Some(val) = lc_core::traits::ToFieldString::to_field_string(&self.#field_name) {
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

/// Re-export `MorphologyInfo` derive from panini-macro.
///
/// This is a wrapper that generates code referencing `panini_core::traits::MorphologyInfo`
/// but also generates a backwards-compatible impl via `lc_core::traits::MorphologyInfo`
/// since `lc_core` re-exports the trait from `panini_core`.
///
/// # Panics
///
/// Panics if the input is not an enum or if any variant is missing a `lemma` field.
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
        assert!(
            has_lemma,
            "MorphologyInfo: variant `{}` must have a named `lemma` field",
            variant.ident
        );
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

        impl lc_core::traits::MorphologyInfo for #name {
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
