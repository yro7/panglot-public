use static_assertions::assert_impl_all;

use crate::provider::AnkiStorageProvider;

assert_impl_all!(AnkiStorageProvider: Send, Sync);
