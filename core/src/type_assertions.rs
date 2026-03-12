use static_assertions::assert_impl_all;

use crate::domain::{CardMetadata, ExtractedFeature};
use crate::traits::{IpaConfig, NoExtraFields, Script, TtsConfig};
use crate::user::{FluencyLevel, KnownLanguage, UserProfile};

// Core domain types
assert_impl_all!(NoExtraFields: Send, Sync);
assert_impl_all!(IpaConfig: Send, Sync);
assert_impl_all!(TtsConfig: Send, Sync);
assert_impl_all!(Script: Send, Sync, Copy, Eq, std::hash::Hash);
assert_impl_all!(FluencyLevel: Send, Sync);
assert_impl_all!(KnownLanguage: Send, Sync);
assert_impl_all!(UserProfile: Send, Sync);

// Generic types — assert with a concrete Morphology that is Send+Sync.
// If these pass, any Morphology that is Send+Sync will also work.
// We use `String` as a simple stand-in.
assert_impl_all!(ExtractedFeature<String>: Send, Sync);
assert_impl_all!(CardMetadata<String>: Send, Sync);
