use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use engine::llm_client::{ChatMessage, LlmClient, LlmHttpClient, LlmProvider, LlmRequest, Role};

const MODEL: &str = "gemini-2.5-flash";

// Example files used as few-shot context for the LLM
const EXAMPLE_RS_FILES: &[&str] = &[
    "langs/src/polish.rs",
    "langs/src/japanese.rs",
    "langs/src/arabic.rs",
];

const EXAMPLE_TREE_FILES: &[&str] = &[
    "core/trees/pol_tree.yaml",
    "core/trees/jpn_tree.yaml",
    "core/trees/ara_tree.yaml",
];

fn read_examples(paths: &[&str]) -> Result<String> {
    let mut buf = String::new();
    for path in paths {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read example file: {path}"))?;
        buf.push_str(&format!("--- {path} ---\n{content}\n\n"));
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
        let rest = if let Some(pos) = rest.find('\n') {
            &rest[pos + 1..]
        } else {
            rest
        };
        if let Some(content) = rest.strip_suffix("```") {
            return content.trim().to_string();
        }
        return rest.trim().to_string();
    }
    s.to_string()
}

async fn generate_rs(client: &LlmHttpClient, iso: &str, lang_name: &str) -> Result<String> {
    let examples = read_examples(EXAMPLE_RS_FILES)?;

    let system = "\
You are an expert computational linguist and Rust developer. \
You generate language definition files for the Panglot project. \
Your output must be ONLY valid Rust code — no markdown fences, no explanations, no comments beyond doc comments. \
The code must compile as-is when placed in the project.";

    let user = format!(
        "Generate a complete Rust file for the language \"{lang_name}\" (ISO 639-3 code: `{iso}`).\n\n\
         Follow EXACTLY the same pattern as these examples:\n\n\
         {examples}\n\n\
         Critical rules:\n\
         - The morphology enum must be named with the language name + \"Morphology\" (PascalCase)\n\
         - The struct must be named with the language name (PascalCase)\n\
         - Every morphology variant MUST have a `lemma: String` field as its first field\n\
         - Use `#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema, lc_macro::MorphologyInfo)]`\n\
         - Use `#[serde(tag = \"pos\")]` and `#[serde(rename_all = \"lowercase\")]` (NOT snake_case)\n\
         - The `iso_code()` must return `lc_core::traits::IsoLang::{iso_variant}` where the variant is the PascalCase of `{iso}`\n\
         - Choose appropriate `IpaConfig` and `TtsConfig` strategies for this language\n\
         - For `typological_features()`, ONLY use known variants: `TypologicalFeature::Conjugation` (do NOT guess others).\n\
         - Include linguistically accurate morphological categories for this language\n\
         - Doc comments on each variant should include the native term in parentheses\n\
         - Use `type ExtraFields = NoExtraFields` unless the language truly needs disambiguation fields\n\n\
         Output ONLY the Rust code, nothing else.",
        iso_variant = capitalize_first(iso),
    );

    let request = LlmRequest {
        messages: vec![
            ChatMessage { role: Role::System, content: system.to_string() },
            ChatMessage { role: Role::User, content: user },
        ],
        temperature: 0.3,
        max_tokens: Some(8000),
        response_schema: None,
    };

    let response = client.chat_completion(&request).await?;
    Ok(strip_code_fences(&response))
}

async fn generate_tree(client: &LlmHttpClient, iso: &str, lang_name: &str) -> Result<String> {
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
         Output ONLY the YAML, nothing else.",
    );

    let request = LlmRequest {
        messages: vec![
            ChatMessage { role: Role::System, content: system.to_string() },
            ChatMessage { role: Role::User, content: user },
        ],
        temperature: 0.5,
        max_tokens: Some(4000),
        response_schema: None,
    };

    let response = client.chat_completion(&request).await?;
    Ok(strip_code_fences(&response))
}

fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase(),
        None => String::new(),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        bail!("Usage: lc-codegen <iso-639-3-code>\nExample: lc-codegen swh");
    }
    let iso = &args[1];

    // Validate ISO code and get language name
    let lang_name = resolve_language_name(iso)?;
    println!("Language: {lang_name} ({iso})");

    // Check files don't already exist
    let rs_path = format!("langs/src/{iso}.rs");
    let tree_path = format!("core/trees/{iso}_tree.yaml");

    if Path::new(&rs_path).exists() {
        bail!("{rs_path} already exists! Delete it first if you want to regenerate.");
    }
    if Path::new(&tree_path).exists() {
        bail!("{tree_path} already exists! Delete it first if you want to regenerate.");
    }

    // Init LLM client — codegen always uses Google/Gemini
    let client = LlmHttpClient::from_provider(LlmProvider::Google, MODEL)
        .context("Failed to init Google LLM client for codegen")?;

    // Step 1: Generate .rs file
    println!("\n[1/2] Generating {rs_path}...");
    let rs_content = generate_rs(&client, iso, &lang_name).await?;
    fs::write(&rs_path, &rs_content)
        .with_context(|| format!("Failed to write {rs_path}"))?;
    println!("  ✅ Written {rs_path}");

    // Step 2: Generate tree yaml
    println!("\n[2/2] Generating {tree_path}...");
    let tree_content = generate_tree(&client, iso, &lang_name).await?;
    fs::write(&tree_path, &tree_content)
        .with_context(|| format!("Failed to write {tree_path}"))?;
    println!("  ✅ Written {tree_path}");

    println!("\n🎉 Done! Run `cargo build` to verify the generated code compiles.");
    println!("   The new language will be auto-registered by the build script.");

    Ok(())
}
