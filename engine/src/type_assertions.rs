use static_assertions::assert_impl_all;

use langs::{Japanese, JapaneseMorphology, Polish, PolishMorphology};

use crate::analyzer::{DynLexiconTracker, LexiconTracker, LibraryAnalyzer, WordProfile};
use crate::card_models::{
    AnyCard, CardModelId, ClozeTest, CommonCardFront, OralComprehension, WrittenComprehension,
};
use crate::generator::GenerationRequest;
use crate::pipeline::{GeneratedCard, Pipeline};
use crate::post_process::{EarlyPostProcessResult, IpaGenerator, TtsGenerator};
use crate::prompts::PromptBuilderError;
use crate::skill_tree::{SkillNode, SkillNodeConfig, SkillTree, SkillTreeConfig};

// ── Analyzer ──
assert_impl_all!(WordProfile<PolishMorphology>: Send, Sync);
assert_impl_all!(LexiconTracker<PolishMorphology>: Send, Sync);
assert_impl_all!(LibraryAnalyzer: Send, Sync);
assert_impl_all!(LexiconTracker<PolishMorphology>: DynLexiconTracker);

// ── Card Models ──
assert_impl_all!(CardModelId: Send, Sync);
assert_impl_all!(AnyCard: Send, Sync);
assert_impl_all!(CommonCardFront: Send, Sync);
assert_impl_all!(ClozeTest: Send, Sync);
assert_impl_all!(WrittenComprehension: Send, Sync);
assert_impl_all!(OralComprehension: Send, Sync);

// ── Generator ──
assert_impl_all!(GenerationRequest<Polish>: Send, Sync);
assert_impl_all!(GenerationRequest<Japanese>: Send, Sync);

// ── Pipeline ──
assert_impl_all!(GeneratedCard<PolishMorphology>: Send, Sync);
assert_impl_all!(GeneratedCard<JapaneseMorphology>: Send, Sync);
assert_impl_all!(Pipeline<Polish>: Send, Sync);
assert_impl_all!(Pipeline<Japanese>: Send, Sync);

// ── Post-Processing ──
assert_impl_all!(EarlyPostProcessResult: Send, Sync);
assert_impl_all!(TtsGenerator: Send, Sync);
assert_impl_all!(IpaGenerator: Send, Sync);

// ── Prompts ──
assert_impl_all!(PromptBuilderError: Send, Sync);

// ── Skill Tree ──
assert_impl_all!(SkillTreeConfig: Send, Sync);
assert_impl_all!(SkillNodeConfig: Send, Sync);
assert_impl_all!(SkillNode: Send, Sync);
assert_impl_all!(SkillTree<Polish>: Send, Sync);
assert_impl_all!(SkillTree<Japanese>: Send, Sync);
