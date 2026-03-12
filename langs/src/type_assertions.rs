use static_assertions::assert_impl_all;

use crate::japanese::{Japanese, JapaneseExtraFields, JapaneseMorphology};
use crate::polish::{Polish, PolishMorphology};

assert_impl_all!(PolishMorphology: Send, Sync);
assert_impl_all!(Polish: Send, Sync);

assert_impl_all!(JapaneseMorphology: Send, Sync);
assert_impl_all!(JapaneseExtraFields: Send, Sync);
assert_impl_all!(Japanese: Send, Sync);
