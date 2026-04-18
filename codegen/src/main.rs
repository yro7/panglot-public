use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use rig::client::CompletionClient as _;
use rig::completion::CompletionModel as _;
use rig::providers::gemini;

const MODEL: &str = "gemini-3-flash-preview";

// ─── Example files for the PANINI implementation ──────────────────────────────
//
// These show what goes in `panini/panini-langs/src/<lang>.rs`.
// We pick a diverse set:
//   - Turkish  → agglutinative (implements Agglutinative trait, rich GrammaticalFunction enum)
//   - Arabic   → Semitic, non-agglutinative but morphologically complex (GrammaticalFunction = ())
//   - French   → Romance, non-agglutinative (GrammaticalFunction = ())
//   - Italian  → Romance, non-agglutinative (GrammaticalFunction = ())
//   - Polish   → Slavic, heavily inflected, non-agglutinative (GrammaticalFunction = ())
const EXAMPLE_PANINI_RS_FILES: &[&str] = &[
    "panini/panini-langs/src/turkish.rs",  // Agglutinative; GrammaticalFunction enum + Agglutinative impl
    "panini/panini-langs/src/arabic.rs",   // Semitic root/pattern; GrammaticalFunction = ()
    "panini/panini-langs/src/french.rs",   // Romance; GrammaticalFunction = ()
    "panini/panini-langs/src/italian.rs",  // Romance; GrammaticalFunction = ()
    "panini/panini-langs/src/polish.rs",   // Slavic inflectional; GrammaticalFunction = ()
];

// ─── Example files for the PANGLOT wrapper ────────────────────────────────────
//
// These show what goes in `langs/src/<iso>.rs`.
// They are thin wrappers that delegate to the panini-langs impl.
const EXAMPLE_PANGLOT_RS_FILES: &[&str] = &[
    "langs/src/tur.rs",     // Wrapper for Turkish (agglutinative, NoExtraFields)
    "langs/src/arabic.rs",  // Wrapper for Arabic (ArabicExtraFields with context_disambiguation)
    "langs/src/polish.rs",  // Wrapper for Polish (NoExtraFields)
];

// ─── Example skill tree YAML files ───────────────────────────────────────────
const EXAMPLE_TREE_FILES: &[&str] = &[
    "core/trees/pol_tree.yaml",
    "core/trees/jpn_tree.yaml",
    "core/trees/ara_tree.yaml",
];

fn read_examples(paths: &[&str]) -> Result<String> {
    use std::fmt::Write;
    let mut buf = String::new();
    for path in paths {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read example file: {path}"))?;
        write!(buf, "--- {path} ---\n{content}\n\n").expect("String write failed");
    }
    Ok(buf)
}

fn resolve_language_name(iso: &str) -> Result<String> {
    let lang = isolang::Language::from_639_3(iso)
        .with_context(|| format!("'{iso}' is not a valid ISO 639-3 code"))?;
    Ok(lang.to_name().to_string())
}

/// Strip ```rust / ``` fences and ```yaml / ``` fences from LLM output
fn strip_code_fences(s: &str) -> String {
    let s = s.trim();
    // Try to strip ```rust or ```yaml or ``` prefix/suffix
    if let Some(rest) = s.strip_prefix("```") {
        // Skip the language tag line
        let rest = rest.find('\n').map_or(rest, |pos| &rest[pos + 1..]);
        if let Some(content) = rest.strip_suffix("```") {
            return content.trim().to_string();
        }
        return rest.trim().to_string();
    }
    s.to_string()
}

/// Generate the PANINI implementation file: `panini/panini-langs/src/<lang_snake>.rs`
///
/// This file defines:
/// - Language-specific morphological category enums (e.g. `XyzCase`, `XyzTense`, …)
/// - Optionally, a `GrammaticalFunction` enum + `Agglutinative` impl for agglutinative languages
/// - The main Morphology enum (`XyzMorphology`) with all POS variants
/// - The `LinguisticDefinition` impl for the unit struct
async fn generate_panini_rs(
    client: &gemini::CompletionModel,
    iso: &str,
    lang_name: &str,
    struct_name: &str,
) -> Result<String> {
    let examples = read_examples(EXAMPLE_PANINI_RS_FILES)?;

    let system = "\
You are an expert computational linguist and Rust developer working on the Pāṇini project. \
You generate language definition files for the panini-langs crate. \
Your output must be ONLY valid Rust code — no markdown fences, no explanations, no prose. \
The code must compile as-is when placed in the project alongside the existing crates.";

    let user = format!(
        "Generate a complete `panini/panini-langs/src/<lang>.rs` file for the language \
\"{lang_name}\" (ISO 639-3 code: `{iso}`, Rust struct name: `{struct_name}`).\n\n\
Study CAREFULLY all five examples below — they cover agglutinative vs. non-agglutinative languages \
and various morphological families:\n\n\
{examples}\n\n\
═══════════════════════════════════════════════════════\n\
ARCHITECTURE RULES (read every line before writing code)\n\
═══════════════════════════════════════════════════════\n\
\n\
1. IMPORTS\n\
   - Always: `use serde::{{Deserialize, Serialize}};`\n\
   - Always: `use panini_core::traits::{{LinguisticDefinition, Script, TypologicalFeature, …}};`\n\
   - Import ONLY the traits/types you actually use from `panini_core::traits`.\n\
   - Common shared types: `BinaryNumber`, `BinaryGender`, `BinaryVoice`, `TernaryNumber`,\n\
     `Person`, `SlavicAspect`. Use them instead of redefining.\n\
   - For agglutinative languages ONLY: also import\n\
     `panini_core::morpheme::{{Agglutinative, MorphemeDefinition, WordSegmentation}}`\n\
     and `panini_core::traits::{{BinaryNumber, Person}}` (as needed).\n\
\n\
2. HELPER ENUMS  (language-specific morphological categories)\n\
   Each helper enum MUST:\n\
   - `#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, \
schemars::JsonSchema, panini_macro::ClosedValues)]`\n\
   - `#[serde(rename_all = \"snake_case\")]`\n\
   - Be named `{struct_name}<Category>` (e.g. `{struct_name}Case`, `{struct_name}Tense`).\n\
   - Have variants with the native term in a line comment (see Turkish example).\n\
   Define ONLY the helper enums that are actually used in the Morphology enum.\n\
\n\
3. GRAMMATICAL FUNCTION ENUM\n\
   - NON-agglutinative: `type GrammaticalFunction = ();` — do NOT define a GrammaticalFunction enum.\n\
   - AGGLUTINATIVE: Define `{struct_name}GrammaticalFunction` tagged with\n\
     `#[serde(tag = \"category\", rename_all = \"snake_case\")]` and add\n\
     `#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, \
panini_macro::AggregableFields)]`.\n\
     Include a `directive_label(&self) -> String` method as shown in the Turkish example.\n\
\n\
4. MORPHOLOGY ENUM (`{struct_name}Morphology`)\n\
   - `#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema, \
panini_macro::MorphologyInfo)]`\n\
   - `#[serde(tag = \"pos\")]`\n\
   - `#[serde(rename_all = \"lowercase\")]`\n\
   - Every variant MUST have `lemma: String` as its FIRST field.\n\
   - Represent all universal POS categories: Adjective, Adposition, Adverb, Auxiliary,\n\
     CoordinatingConjunction, Determiner, Interjection, Noun, Numeral, Particle,\n\
     Pronoun, ProperNoun, Punctuation, SubordinatingConjunction, Symbol, Verb, Other.\n\
   - Add ONLY the morphological fields that are linguistically required for EACH variant.\n\
\n\
5. AGGLUTINATIVE LANGUAGES ONLY\n\
   Is `{lang_name}` agglutinative (e.g. Turkish, Finnish, Hungarian, Japanese, Korean,\n\
   Swahili, Quechua, Georgian, Basque, Mongolian)? \n\
   YES → You MUST:\n\
     a. Define `{struct_name}GrammaticalFunction` and `{struct_name}Morpheme*` helper enums.\n\
     b. Define a `static {lang_name_upper}_MORPHEMES: &[MorphemeDefinition<F, P>]` array.\n\
     c. `impl Agglutinative for {struct_name}` with `morpheme_inventory()` and\n\
        `morpheme_directives(&self) -> String` (following the Turkish template exactly).\n\
     d. In the `LinguisticDefinition` impl, call `extra_extraction_directives` returning\n\
        `Some(self.morpheme_directives())`.\n\
   NO → `type GrammaticalFunction = ();`. Do NOT implement `Agglutinative`.\n\
\n\
6. `LinguisticDefinition` IMPL\n\
   ```rust\n\
   pub struct {struct_name};\n\
   impl LinguisticDefinition for {struct_name} {{\n\
       type Morphology = {struct_name}Morphology;\n\
       type GrammaticalFunction = /* () or {struct_name}GrammaticalFunction */;\n\
       const ISO_CODE: &'static str = \"{iso}\";\n\
       fn supported_scripts(&self) -> &[Script] {{ &[Script::XXXX] }}\n\
       fn default_script(&self) -> Script {{ Script::XXXX }}\n\
       fn typological_features(&self) -> &[TypologicalFeature] {{\n\
           &[TypologicalFeature::Conjugation /* + Agglutination if agglutinative */]\n\
       }}\n\
       fn extraction_directives(&self) -> &'static str {{\n\
           \"Detailed, linguistically-accurate directives for this specific language.\"\n\
       }}\n\
       // For agglutinative ONLY:\n\
       fn extra_extraction_directives(&self) -> Option<String> {{\n\
           Some(self.morpheme_directives())\n\
       }}\n\
       // For agglutinative ONLY:\n\
       fn post_process_extraction(&self, segmentation: &mut Option<Vec<WordSegmentation<{struct_name}GrammaticalFunction>>>) -> Result<(), String> {{\n\
           self.validate_and_enrich(segmentation)\n\
       }}\n\
   }}\n\
   ```\n\
   - `typological_features`: use ONLY known variants: `TypologicalFeature::Conjugation`,\n\
     `TypologicalFeature::Agglutination`. Do NOT invent others.\n\
   - `extraction_directives`: write precise, linguistically-accurate numbered rules.\n\
     Mention language-specific phenomena (e.g. vowel harmony, ergativity, classifier usage,\n\
     triconsonantal roots, etc.) as relevant.\n\
\n\
Output ONLY the Rust code, nothing else.",
        lang_name_upper = lang_name.to_uppercase().replace(' ', "_"),
    );

    let raw = client.completion_request(&user)
        .preamble(system.to_string())
        .temperature(0.2)
        .max_tokens(10000)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("LLM request failed: {e}"))?
        .choice
        .into_iter()
        .find_map(|c| {
            if let rig::completion::message::AssistantContent::Text(t) = c { Some(t.text) } else { None }
        })
        .ok_or_else(|| anyhow::anyhow!("LLM returned no text"))?;
    Ok(strip_code_fences(&raw))
}

/// Generate the PANGLOT wrapper file: `langs/src/<iso>.rs`
///
/// This file is a thin Panglot wrapper that:
/// - `pub use panini_langs::<module>::*;`
/// - Defines a local unit struct with the same name, shadowing the glob import
/// - Implements `Language` using `lc_core::import_from_panini!(panini_langs::<module>::<Struct>)`
/// - Fills in `ExtraFields`, `generation_directives`, `ipa_strategy`, `tts_strategy`
async fn generate_panglot_rs(
    client: &gemini::CompletionModel,
    iso: &str,
    lang_name: &str,
    struct_name: &str,
    module_name: &str,
    panini_content: &str,
) -> Result<String> {
    let examples = read_examples(EXAMPLE_PANGLOT_RS_FILES)?;

    let system = "\
You are an expert Rust developer working on the Panglot project. \
You generate thin wrapper files for the `langs` crate that delegate to panini-langs. \
Your output must be ONLY valid Rust code — no markdown fences, no explanations, no prose.";

    let user = format!(
        "Generate a complete `langs/src/{iso}.rs` wrapper file for the language \
\"{lang_name}\" (ISO 639-3: `{iso}`, module: `{module_name}`, struct: `{struct_name}`).\n\n\
Here is the PANINI implementation that was just generated for this language \
(in `panini/panini-langs/src/{module_name}.rs`). You MUST base your wrapper on it:\n\n\
--- panini/panini-langs/src/{module_name}.rs ---\n\
{panini_content}\n\n\
Study CAREFULLY the three example wrapper files below:\n\n\
{examples}\n\
═══════════════════════════════════════════════════════\n\
WRAPPER FILE RULES\n\
═══════════════════════════════════════════════════════\n\
\n\
1. GLOB RE-EXPORT\n\
   ```rust\n\
   pub use panini_langs::{module_name}::*;\n\
   ```\n\
   This pulls in the Morphology enum, helper types, etc. from panini-langs.\n\
\n\
2. IMPORTS\n\
   - Always: `use lc_core::traits::{{IpaConfig, Language, TtsConfig}};`\n\
   - If ExtraFields needed: also `use serde::{{Deserialize, Serialize}};`\n\
   - Use `NoExtraFields` (from `lc_core::traits`) if no extra fields are needed.\n\
\n\
3. EXTRA FIELDS\n\
   - Does `{lang_name}` benefit from an extra field in LLM generation output\n\
     (e.g. diacritized/vowelled form for Arabic, transliteration for a non-Latin script language)?\n\
   - YES → Define `pub struct {struct_name}ExtraFields {{ … }}` with appropriate fields,\n\
     `#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]`.\n\
     See `arabic.rs` for the pattern.\n\
   - NO  → Use `type ExtraFields = NoExtraFields;`.\n\
\n\
4. LOCAL WRAPPER STRUCT + Language IMPL\n\
   ```rust\n\
   /// Local wrapper — shadows the glob-imported `{struct_name}` from panini-langs.\n\
   /// Delegates linguistic definition to `panini_langs::{module_name}::{struct_name}` via composition.\n\
   pub struct {struct_name};\n\
\n\
   impl Language for {struct_name} {{\n\
       lc_core::import_from_panini!(panini_langs::{module_name}::{struct_name});\n\
       type ExtraFields = /* NoExtraFields or {struct_name}ExtraFields */;\n\
\n\
       fn generation_directives(&self) -> Option<&str> {{\n\
           Some(\"<linguistically accurate generation instructions for {lang_name}>\")\n\
       }}\n\
\n\
       fn ipa_strategy(&self) -> IpaConfig {{\n\
           IpaConfig::Epitran(\"<epitran-code>\") // or IpaConfig::None\n\
       }}\n\
\n\
       fn tts_strategy(&self) -> TtsConfig {{\n\
           TtsConfig::Edge {{ voice: \"<azure-voice-name>\" }} // or TtsConfig::None\n\
       }}\n\
   }}\n\
   ```\n\
\n\
5. GENERATION DIRECTIVES\n\
   Write concise but linguistically accurate natural-language instructions for the LLM\n\
   generator (not extractor). Focus on:\n\
   - Script/orthography expectations\n\
   - Pro-drop, SOV/SVO, etc.\n\
   - Agreement rules specific to {lang_name}\n\
   - Anything a learner-facing system must get right\n\
\n\
6. IPA STRATEGY\n\
   - If Epitran supports `{lang_name}`, use `IpaConfig::Epitran(\"<lang_script_code>\")`\n\
     (e.g. \"tur-Latn\", \"ara-Arab\", \"pol-Latn\", \"hin-Deva\").\n\
   - Otherwise use `IpaConfig::None`.\n\
\n\
7. TTS STRATEGY\n\
   - Choose the best matching Azure Neural Voice for `{lang_name}` if one exists.\n\
     Pattern: `\"<lang_region>-<Name>Neural\"` (e.g. \"tr-TR-AhmetNeural\").\n\
   - Otherwise use `TtsConfig::None`.\n\
\n\
Output ONLY the Rust code, nothing else.",
        examples = examples,
    );

    let raw = client.completion_request(&user)
        .preamble(system.to_string())
        .temperature(0.2)
        .max_tokens(4000)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("LLM request failed: {e}"))?
        .choice
        .into_iter()
        .find_map(|c| {
            if let rig::completion::message::AssistantContent::Text(t) = c { Some(t.text) } else { None }
        })
        .ok_or_else(|| anyhow::anyhow!("LLM returned no text"))?;
    Ok(strip_code_fences(&raw))
}

async fn generate_tree(client: &gemini::CompletionModel, iso: &str, lang_name: &str) -> Result<String> {
    let examples = read_examples(EXAMPLE_TREE_FILES)?;

    let system = "\
You are an expert language teacher and curriculum designer. \
You generate skill tree YAML files for the Panglot language learning project. \
Your output must be ONLY valid YAML — no markdown fences, no explanations.";

    let user = format!(
        "Generate a skill tree YAML file for \"{lang_name}\" (ISO: `{iso}`).\n\n\
         Follow EXACTLY the same structure as these examples:\n\n\
         {examples}\n\n\
         Rules:\n\
         - `language_name` must be \"{lang_name}\"\n\
         - The root node's `name` should be the language's native name for itself\n\
         - Include 3-5 main categories (grammar, vocabulary, pronunciation, etc.) appropriate for this language\n\
         - Each category should have 2-4 leaf nodes with `node_instructions`\n\
         - `node_instructions` should describe what kind of exercise to generate (cloze, translation, etc.)\n\
         - Focus on features that are distinctive or challenging for this language\n\
         - All leaf nodes must have `children: []`\n\n\
         Prerequisites (optional but encouraged):\n\
         - Any node may declare a `prerequisites: [id1, id2]` field listing IDs of other nodes\n\
           that should be learned first. Use this to encode pedagogical ordering (e.g. past\n\
           tense requires present tense, accusative requires nominative).\n\
         - Unknown or cyclic prereq IDs are tolerated silently, but keep the graph clean.\n\
         - Example of a node with prereqs:\n\
           `  - id: past_tense`\n\
           `    name: Past Tense`\n\
           `    node_instructions: Generate a past-tense conjugation cloze.`\n\
           `    prerequisites: [present_tense]`\n\
           `    children: []`\n\n\
         Output ONLY the YAML, nothing else.",
    );

    let raw = client.completion_request(&user)
        .preamble(system.to_string())
        .temperature(0.5)
        .max_tokens(4000)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("LLM request failed: {e}"))?
        .choice
        .into_iter()
        .find_map(|c| {
            if let rig::completion::message::AssistantContent::Text(t) = c { Some(t.text) } else { None }
        })
        .ok_or_else(|| anyhow::anyhow!("LLM returned no text"))?;
    Ok(strip_code_fences(&raw))
}

fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    chars.next().map_or_else(String::new, |c| {
        c.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()
    })
}

/// Derive the Rust struct name from the language name.
/// E.g. "Turkish" → "Turkish", "Modern Standard Arabic" → "ModernStandardArabic"
fn to_struct_name(lang_name: &str) -> String {
    lang_name
        .split_whitespace()
        .map(capitalize_first)
        .collect::<String>()
}

/// Derive the snake_case module name for panini-langs from the language name.
/// E.g. "Turkish" → "turkish", "Modern Standard Arabic" → "modern_standard_arabic"
fn to_module_name(lang_name: &str) -> String {
    lang_name
        .split_whitespace()
        .map(|w| w.to_lowercase())
        .collect::<Vec<_>>()
        .join("_")
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        bail!("Usage: lc-codegen <iso-639-3-code>\nExample: lc-codegen swh\nBuild: cargo run -p lc-codegen <iso-639-3-code>");
    }
    let iso = &args[1];

    // Validate ISO code and get language name
    let lang_name = resolve_language_name(iso)?;
    let struct_name = to_struct_name(&lang_name);
    let module_name = to_module_name(&lang_name);
    println!("Language:    {lang_name} ({iso})");
    println!("Struct name: {struct_name}");
    println!("Module name: {module_name}");

    // Output file paths
    let panini_path = format!("panini/panini-langs/src/{module_name}.rs");
    let panglot_path = format!("langs/src/{iso}.rs");
    let tree_path = format!("core/trees/{iso}_tree.yaml");

    // Guard: refuse to overwrite existing files
    for path in [&panini_path, &panglot_path, &tree_path] {
        if Path::new(path).exists() {
            bail!("{path} already exists! Delete it first if you want to regenerate.");
        }
    }

    // Init LLM client — codegen always uses Google/Gemini
    let api_key = std::env::var("GOOGLE_API_KEY").context("GOOGLE_API_KEY not set")?;
    let client = gemini::Client::new(&api_key)
        .map_err(|e| anyhow::anyhow!("Failed to init Gemini client: {e}"))?
        .completion_model(MODEL);

    // ── Step 1: Generate the PANINI implementation ────────────────────────────
    println!("\n[1/3] Generating {panini_path}...");
    let panini_content = generate_panini_rs(&client, iso, &lang_name, &struct_name).await?;
    fs::write(&panini_path, &panini_content)
        .with_context(|| format!("Failed to write {panini_path}"))?;
    println!("  ✅ Written {panini_path}");

    // ── Step 2: Generate the PANGLOT wrapper ──────────────────────────────────
    // We pass the generated panini file as context so the LLM knows exactly
    // which types, enums and module name to reference.
    println!("\n[2/3] Generating {panglot_path}...");
    let panglot_content = generate_panglot_rs(
        &client,
        iso,
        &lang_name,
        &struct_name,
        &module_name,
        &panini_content,
    )
    .await?;
    fs::write(&panglot_path, &panglot_content)
        .with_context(|| format!("Failed to write {panglot_path}"))?;
    println!("  ✅ Written {panglot_path}");

    // ── Step 3: Generate the skill tree YAML ─────────────────────────────────
    println!("\n[3/3] Generating {tree_path}...");
    let tree_content = generate_tree(&client, iso, &lang_name).await?;
    fs::write(&tree_path, &tree_content)
        .with_context(|| format!("Failed to write {tree_path}"))?;
    println!("  ✅ Written {tree_path}");

    println!("\n🎉 Done! Next steps:");
    println!("   1. Add `pub mod {module_name};` and `pub use {module_name}::*;` to");
    println!("      `panini/panini-langs/src/lib.rs`");
    println!("   2. Add `{struct_name}` to the `generate_registry!(…)` call in");
    println!("      `panini/panini-langs/src/registry.rs`");
    println!("   3. Run `cargo build` to verify the generated code compiles.");
    println!("      The new language will be auto-registered by the langs build script.");

    Ok(())
}
