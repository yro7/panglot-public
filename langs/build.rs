use std::fs;
use std::path::Path;

use regex::Regex;

fn main() {
    println!("cargo:rerun-if-changed=src/");
    println!("cargo:rerun-if-changed=../panini/panini-langs/src/");

    let src_dir = Path::new("src");
    let panini_langs_dir = Path::new("../panini/panini-langs/src");
    let skip = ["lib.rs", "type_assertions.rs"];

    let struct_re = Regex::new(r"impl\s+Language\s+for\s+(\w+)").unwrap();
    let morph_re = Regex::new(r"type\s+Morphology\s*=\s*(\w+)").unwrap();
    let iso_re = Regex::new(r"IsoLang::(\w+)").unwrap();
    let extra_re = Regex::new(r"type\s+ExtraFields\s*=\s*(\w+)").unwrap();
    let gf_re = Regex::new(r"type\s+GrammaticalFunction\s*=\s*([\w()]+)").unwrap();
    let panini_re = Regex::new(r"pub\s+use\s+panini_langs::(\w+)::\*").unwrap();

    // For panini-langs files, look for `impl LinguisticDefinition for X`
    let ld_struct_re = Regex::new(r"impl\s+LinguisticDefinition\s+for\s+(\w+)").unwrap();

    struct LangInfo {
        mod_name: String,
        struct_name: String,
        morphology_name: String,
        extra_fields_name: Option<String>,
        grammatical_function_name: Option<String>,
        iso_lower: String,
    }

    let mut langs = Vec::new();

    for entry in fs::read_dir(src_dir).expect("cannot read langs/src/") {
        let entry = entry.unwrap();
        let filename = entry.file_name().to_string_lossy().to_string();
        if !filename.ends_with(".rs") || skip.contains(&filename.as_str()) {
            continue;
        }

        let mod_name = filename.trim_end_matches(".rs").to_string();
        let content = fs::read_to_string(entry.path()).unwrap();

        // Only process files that implement Language
        let struct_name = match struct_re.captures(&content) {
            Some(cap) => cap.get(1).unwrap().as_str().to_string(),
            None => continue,
        };

        // Check if this file re-exports from panini-langs
        let is_panini = panini_re.is_match(&content);

        // For panini-langs re-exports, read the Morphology type and ISO code
        // from the panini-langs source file instead
        let (morphology_name, iso_variant, grammatical_function) = if is_panini {
            let panini_mod = panini_re.captures(&content).unwrap()
                .get(1).unwrap().as_str();
            let panini_file = panini_langs_dir.join(format!("{panini_mod}.rs"));
            let panini_content = fs::read_to_string(&panini_file)
                .unwrap_or_else(|_| panic!("Cannot read panini-langs file: {}", panini_file.display()));

            let morph = morph_re.captures(&panini_content)
                .or_else(|| {
                    // In panini-langs, Morphology is on LinguisticDefinition, not Language
                    Regex::new(r"type\s+Morphology\s*=\s*(\w+)").unwrap().captures(&panini_content)
                })
                .unwrap_or_else(|| panic!("No Morphology type found in panini-langs/{panini_mod}.rs"))
                .get(1).unwrap().as_str().to_string();

            let iso = iso_re.captures(&panini_content)
                .unwrap_or_else(|| panic!("No IsoLang found in panini-langs/{panini_mod}.rs"))
                .get(1).unwrap().as_str().to_string();

            let gf = gf_re.captures(&panini_content)
                .map(|c| c.get(1).unwrap().as_str().to_string())
                .and_then(|name| if name == "()" { None } else { Some(name) });

            (morph, iso, gf)
        } else {
            let morph = morph_re.captures(&content)
                .unwrap_or_else(|| panic!("No `type Morphology = X` found in {filename}"))
                .get(1).unwrap().as_str().to_string();

            let iso = iso_re.captures(&content)
                .unwrap_or_else(|| panic!("No `IsoLang::Xxx` found in {filename}"))
                .get(1).unwrap().as_str().to_string();

            let gf = gf_re.captures(&content)
                .map(|c| c.get(1).unwrap().as_str().to_string())
                .and_then(|name| if name == "()" { None } else { Some(name) });

            (morph, iso, gf)
        };

        let extra_fields = extra_re
            .captures(&content)
            .map(|c| c.get(1).unwrap().as_str().to_string())
            .and_then(|name| {
                if name == "NoExtraFields" {
                    None
                } else {
                    Some(name)
                }
            });

        let iso_lower = iso_variant.to_lowercase();

        langs.push(LangInfo {
            mod_name,
            struct_name,
            morphology_name,
            extra_fields_name: extra_fields,
            grammatical_function_name: grammatical_function,
            iso_lower,
        });
    }

    langs.sort_by(|a, b| a.mod_name.cmp(&b.mod_name));

    // Verify every language has a matching tree YAML in core/trees/
    for l in &langs {
        let tree = format!("../core/trees/{}_tree.yaml", l.iso_lower);
        if !Path::new(&tree).exists() {
            panic!("missing skill-tree file for language '{}': expected {tree}", l.iso_lower);
        }
    }

    // ── Generate lang_registry.rs ──
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let dest = Path::new(&out_dir).join("lang_registry.rs");

    let mut code = String::new();
    code.push_str("// Auto-generated by langs/build.rs — do not edit\n\n");

    // mod declarations with #[path] so they resolve from src/, not OUT_DIR
    for l in &langs {
        code.push_str(&format!(
            "#[path = \"{manifest_dir}/src/{}.rs\"]\npub mod {};\n",
            l.mod_name, l.mod_name,
        ));
    }
    code.push('\n');

    // pub use declarations
    for l in &langs {
        let extra = match &l.extra_fields_name {
            Some(name) => format!(", {name}"),
            None => String::new(),
        };
        let gf = match &l.grammatical_function_name {
            Some(name) => format!(", {name}"),
            None => String::new(),
        };
        code.push_str(&format!(
            "pub use {}::{{{}, {}{extra}{gf}}};\n",
            l.mod_name, l.struct_name, l.morphology_name,
        ));
    }
    code.push('\n');

    // ALL_ISO_CODES constant
    let iso_list = langs.iter()
        .map(|l| format!("\"{}\"", l.iso_lower))
        .collect::<Vec<_>>()
        .join(", ");
    code.push_str(&format!("pub const ALL_ISO_CODES: &[&str] = &[{}];\n\n", iso_list));

    // dispatch_iso! macro
    code.push_str("#[macro_export]\nmacro_rules! dispatch_iso {\n");
    code.push_str("    ($iso:expr, $lang:ident => $body:expr) => {\n");
    code.push_str("        match $iso {\n");
    for l in &langs {
        code.push_str(&format!(
            "            \"{}\" => {{ let $lang = $crate::{}; Some($body) }},\n",
            l.iso_lower, l.struct_name,
        ));
    }
    code.push_str("            _ => None,\n");
    code.push_str("        }\n");
    code.push_str("    };\n");
    code.push_str("}\n");

    fs::write(&dest, code).expect("failed to write lang_registry.rs");
}
