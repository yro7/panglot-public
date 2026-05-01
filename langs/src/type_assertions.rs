use static_assertions::assert_impl_all;

use crate::arabic::{Arabic, ArabicMorphology};
use crate::polish::{Polish, PolishMorphology};
use crate::tur::{Turkish, TurkishMorphology};

assert_impl_all!(ArabicMorphology: Send, Sync);
assert_impl_all!(Arabic: Send, Sync);
assert_impl_all!(PolishMorphology: Send, Sync);
assert_impl_all!(Polish: Send, Sync);
assert_impl_all!(TurkishMorphology: Send, Sync);
assert_impl_all!(Turkish: Send, Sync);
