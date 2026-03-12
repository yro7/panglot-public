use static_assertions::assert_impl_all;

use crate::deck_builder::DeckBuilder;
use crate::provider::AnkiStorageProvider;

assert_impl_all!(AnkiStorageProvider: Send, Sync);
assert_impl_all!(DeckBuilder: Send, Sync);
