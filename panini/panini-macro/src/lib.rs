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

// ─── PaniniResult derive macro ────────────────────────────────────────────────

#[proc_macro_derive(PaniniResult, attributes(component))]
pub fn panini_result_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let generics = &input.generics;

    // We expect EXACTLY one generic parameter L (the language type) for now.
    let type_params: Vec<_> = generics.type_params().collect();
    if type_params.len() != 1 {
        panic!("PaniniResult: struct must have exactly one type parameter (the language type L). Support for multiple generics requires implementing a #[panini(lang = \"L\")] attribute.");
    }
    let lang_ident = &type_params[0].ident;

    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(f) => &f.named,
            _ => panic!("PaniniResult: only named fields are supported"),
        },
        _ => panic!("PaniniResult: can only be derived for structs"),
    };

    // Parse #[component(ComponentName)] attributes on each field.
    // Collect: (field_ident, component_path, is_option)
    let mut component_fields = Vec::new();

    for field in fields {
        let field_ident = field.ident.as_ref().unwrap();
        let mut component_path: Option<syn::Path> = None;

        for attr in &field.attrs {
            if attr.path().is_ident("component") {
                let path: syn::Path = attr.parse_args().expect(
                    "PaniniResult: #[component(Name)] expects a component type path",
                );
                component_path = Some(path);
            }
        }

        let Some(comp_path) = component_path else {
            panic!(
                "PaniniResult: field `{}` must have a #[component(Name)] attribute",
                field_ident
            );
        };

        // Detect if the field type is Option<T>
        let is_option = is_option_type(&field.ty);

        component_fields.push((field_ident.clone(), comp_path, is_option));
    }

    // Generate component instantiation: one let binding per unique component
    let component_lets: Vec<_> = component_fields
        .iter()
        .enumerate()
        .map(|(i, (_, path, _))| {
            let var = quote::format_ident!("__comp_{}", i);
            quote! { let #var = <#path as ::std::default::Default>::default(); }
        })
        .collect();

    // Generate the Vec<&dyn AnalysisComponent<L>> for extract_with_components
    let component_refs: Vec<_> = (0..component_fields.len())
        .map(|i| {
            let var = quote::format_ident!("__comp_{}", i);
            quote! { &#var as &dyn ::panini::__macro_support::panini_core::component::AnalysisComponent<#lang_ident> }
        })
        .collect();

    // Generate deserialization for each field
    let field_deserializations: Vec<_> = component_fields
        .iter()
        .enumerate()
        .map(|(i, (field_ident, _, is_option))| {
            let var = quote::format_ident!("__comp_{}", i);
            let key_expr = quote! { ::panini::__macro_support::panini_core::component::AnalysisComponent::<#lang_ident>::schema_key(&#var) };

            if *is_option {
                // Option field: None if key missing, error on deserialization error
                quote! {
                    #field_ident: match __raw.get(#key_expr) {
                        Ok(val) => Some(val),
                        Err(::panini::__macro_support::panini_core::component::ExtractionResultError::KeyNotFound { .. }) => None,
                        Err(e) => return Err(::panini::__macro_support::panini_engine::extractor::ExtractionError::ResultMapping(e)),
                    }
                }
            } else {
                // Required field: error if key missing or deserialization fails
                quote! {
                    #field_ident: __raw.get(#key_expr).map_err(
                        ::panini::__macro_support::panini_engine::extractor::ExtractionError::ResultMapping
                    )?
                }
            }
        })
        .collect();

    // Generate ComponentRequires<L> bounds for each component
    let requires_bounds: Vec<_> = component_fields
        .iter()
        .map(|(_, path, _)| {
            quote! { #path: ::panini::__macro_support::panini_core::component::ComponentRequires<#lang_ident> }
        })
        .collect();

    // Collect existing where clause predicates
    let existing_where = generics
        .where_clause
        .as_ref()
        .map(|w| {
            let preds = &w.predicates;
            quote! { #preds, }
        })
        .unwrap_or_default();

    let (impl_generics, ty_generics, _) = generics.split_for_impl();

    let expanded = quote! {
        impl #impl_generics #name #ty_generics
        where
            #existing_where
            #lang_ident: ::panini::__macro_support::panini_core::traits::LinguisticDefinition + Send + Sync,
            #(#requires_bounds,)*
        {
            /// Extract features from text using an LLM, returning this typed result struct.
            ///
            /// Components are determined by the struct's `#[component(...)]` annotations.
            /// The extraction pipeline (schema composition, prompt assembly, LLM call,
            /// validation, post-processing) is handled by `extract_with_components` under the hood.
            pub async fn extract<__M: ::panini::__macro_support::rig::completion::CompletionModel>(
                language: &#lang_ident,
                model: &__M,
                request: &::panini::__macro_support::panini_engine::prompts::ExtractionRequest,
                options: ::panini::__macro_support::panini_engine::extractor::ExtractionOptions<'_>,
            ) -> Result<Self, ::panini::__macro_support::panini_engine::extractor::ExtractionError> {
                // Instantiate components using Default implementation
                #(#component_lets)*

                let __components: Vec<&dyn ::panini::__macro_support::panini_core::component::AnalysisComponent<#lang_ident>> = vec![
                    #(#component_refs,)*
                ];

                // Call the untyped extraction pipeline
                let __raw = ::panini::__macro_support::panini_engine::extractor::extract_with_components(
                    language,
                    model,
                    request,
                    &__components,
                    options,
                ).await?;

                // Deserialize each field from the raw result
                Ok(Self {
                    #(#field_deserializations,)*
                })
            }
        }
    };

    TokenStream::from(expanded)
}

/// Check if a `syn::Type` is `Option<T>`.
fn is_option_type(ty: &syn::Type) -> bool {
    if let syn::Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            return segment.ident == "Option";
        }
    }
    false
}
