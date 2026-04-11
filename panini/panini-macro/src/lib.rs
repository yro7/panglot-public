mod helpers;
mod closed_values;
mod morphology_info;
mod panini_result;
mod aggregable_fields;

use proc_macro::TokenStream;

#[proc_macro_derive(ClosedValues, attributes(closed_values))]
pub fn closed_values_derive(input: TokenStream) -> TokenStream {
    closed_values::derive(input)
}

#[proc_macro_derive(MorphologyInfo)]
pub fn morphology_info_derive(input: TokenStream) -> TokenStream {
    morphology_info::derive(input)
}

#[proc_macro_derive(PaniniResult, attributes(component))]
pub fn panini_result_derive(input: TokenStream) -> TokenStream {
    panini_result::derive(input)
}

#[proc_macro_derive(AggregableFields, attributes(aggregable_fields))]
pub fn aggregable_fields_derive(input: TokenStream) -> TokenStream {
    aggregable_fields::derive(input)
}
