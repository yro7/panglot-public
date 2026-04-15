use std::collections::HashMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

use quote::ToTokens;
use syn::{Expr, ExprLit, File, ImplItem, Item, Lit, Type, UseTree};

struct LangInfo {
    mod_name: String,
    struct_name: String,
    morphology_name: String,
    extra_fields_name: Option<String>,
    grammatical_function_name: Option<String>,
    iso_lower: String,
}

struct LocalImpl {
    struct_name: String,
    extra_fields: Option<String>,
    panini_mod: Option<String>,
    morphology: Option<String>,
    grammatical_function: Option<String>,
    iso_variant: Option<String>,
}

struct PaniniImpl {
    morphology: String,
    grammatical_function: Option<String>,
    iso_code: String,
}

fn fail(msg: impl AsRef<str>) -> ! {
    eprintln!("langs/build.rs: {}", msg.as_ref());
    process::exit(1);
}

fn type_to_ident(ty: &Type) -> Option<String> {
    match ty {
        Type::Path(tp) => tp.path.segments.last().map(|s| s.ident.to_string()),
        Type::Tuple(t) if t.elems.is_empty() => Some("()".to_string()),
        _ => Some(ty.to_token_stream().to_string()),
    }
}

fn str_lit(expr: &Expr) -> Option<String> {
    if let Expr::Lit(ExprLit { lit: Lit::Str(s), .. }) = expr {
        Some(s.value())
    } else {
        None
    }
}

fn trait_name(imp: &syn::ItemImpl) -> Option<String> {
    imp.trait_
        .as_ref()
        .and_then(|(_, path, _)| path.segments.last().map(|s| s.ident.to_string()))
}

fn self_ident(imp: &syn::ItemImpl) -> Option<String> {
    if let Type::Path(tp) = &*imp.self_ty {
        tp.path.segments.last().map(|s| s.ident.to_string())
    } else {
        None
    }
}

fn parse_local(file: &File, filename: &str) -> Option<LocalImpl> {
    let mut panini_mod = None;
    for item in &file.items {
        if let Item::Use(u) = item
            && let UseTree::Path(p) = &u.tree
            && p.ident == "panini_langs"
            && let UseTree::Path(inner) = &*p.tree
        {
            panini_mod = Some(inner.ident.to_string());
        }
    }

    for item in &file.items {
        if let Item::Impl(imp) = item {
            if trait_name(imp).as_deref() != Some("Language") {
                continue;
            }
            let struct_name = self_ident(imp).unwrap_or_else(|| {
                fail(format!("{filename}: cannot resolve Self type of `impl Language`"))
            });

            let mut extra_fields = None;
            let mut morphology = None;
            let mut gf = None;
            let mut iso_variant = None;

            for ii in &imp.items {
                match ii {
                    ImplItem::Type(t) => {
                        let name = t.ident.to_string();
                        let ty = type_to_ident(&t.ty).unwrap_or_default();
                        match name.as_str() {
                            "ExtraFields" => extra_fields = Some(ty),
                            "Morphology" => morphology = Some(ty),
                            "GrammaticalFunction" => gf = Some(ty),
                            _ => {}
                        }
                    }
                    ImplItem::Const(c) if c.ident == "ISO_CODE" => {
                        iso_variant = str_lit(&c.expr);
                    }
                    _ => {}
                }
            }

            return Some(LocalImpl {
                struct_name,
                extra_fields,
                panini_mod,
                morphology,
                grammatical_function: gf,
                iso_variant,
            });
        }
    }
    None
}

fn parse_panini(file: &File, path: &Path) -> PaniniImpl {
    for item in &file.items {
        if let Item::Impl(imp) = item {
            if trait_name(imp).as_deref() != Some("LinguisticDefinition") {
                continue;
            }
            let mut morphology = None;
            let mut gf = None;
            let mut iso_code = None;
            for ii in &imp.items {
                match ii {
                    ImplItem::Type(t) => {
                        let name = t.ident.to_string();
                        let ty = type_to_ident(&t.ty).unwrap_or_default();
                        match name.as_str() {
                            "Morphology" => morphology = Some(ty),
                            "GrammaticalFunction" => gf = Some(ty),
                            _ => {}
                        }
                    }
                    ImplItem::Const(c) if c.ident == "ISO_CODE" => {
                        iso_code = str_lit(&c.expr);
                    }
                    _ => {}
                }
            }
            return PaniniImpl {
                morphology: morphology.unwrap_or_else(|| {
                    fail(format!("{}: missing `type Morphology`", path.display()))
                }),
                grammatical_function: gf,
                iso_code: iso_code.unwrap_or_else(|| {
                    fail(format!("{}: missing `const ISO_CODE`", path.display()))
                }),
            };
        }
    }
    fail(format!("{}: no `impl LinguisticDefinition for _` found", path.display()));
}

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let src_dir = manifest_dir.join("src");
    let panini_langs_dir = manifest_dir.join("../panini/panini-langs/src");
    let trees_dir = manifest_dir.join("../core/trees");

    println!("cargo:rerun-if-changed=src/");
    println!("cargo:rerun-if-changed={}", panini_langs_dir.display());
    println!("cargo:rerun-if-changed={}", trees_dir.display());

    let skip = ["lib.rs", "type_assertions.rs"];
    let mut langs: Vec<LangInfo> = Vec::new();
    let mut seen_iso: HashMap<String, String> = HashMap::new();

    let entries = fs::read_dir(&src_dir)
        .unwrap_or_else(|e| fail(format!("cannot read {}: {e}", src_dir.display())));

    for entry in entries {
        let entry = entry.unwrap();
        let filename = entry.file_name().to_string_lossy().to_string();
        if !filename.ends_with(".rs") || skip.contains(&filename.as_str()) {
            continue;
        }

        let mod_name = filename.trim_end_matches(".rs").to_string();
        let path = entry.path();
        let content = fs::read_to_string(&path)
            .unwrap_or_else(|e| fail(format!("cannot read {}: {e}", path.display())));
        let file = syn::parse_file(&content)
            .unwrap_or_else(|e| fail(format!("cannot parse {}: {e}", path.display())));

        let Some(local) = parse_local(&file, &filename) else {
            println!(
                "cargo:warning=langs: skipping {filename} (no `impl Language for _` found)"
            );
            continue;
        };

        let (morphology_name, grammatical_function_name, iso_variant) =
            if let Some(panini_mod) = &local.panini_mod {
                let panini_file = panini_langs_dir.join(format!("{panini_mod}.rs"));
                let panini_content = fs::read_to_string(&panini_file).unwrap_or_else(|e| {
                    fail(format!("cannot read {}: {e}", panini_file.display()))
                });
                let panini_ast = syn::parse_file(&panini_content).unwrap_or_else(|e| {
                    fail(format!("cannot parse {}: {e}", panini_file.display()))
                });
                let info = parse_panini(&panini_ast, &panini_file);
                let gf = info.grammatical_function.filter(|n| n != "()");
                (info.morphology, gf, info.iso_code)
            } else {
                let morph = local.morphology.unwrap_or_else(|| {
                    fail(format!("{filename}: missing `type Morphology`"))
                });
                let iso = local.iso_variant.unwrap_or_else(|| {
                    fail(format!("{filename}: missing `const ISO_CODE`"))
                });
                let gf = local.grammatical_function.filter(|n| n != "()");
                (morph, gf, iso)
            };

        let extra_fields = local.extra_fields.filter(|n| n != "NoExtraFields");
        let iso_lower = iso_variant.to_lowercase();

        if let Some(prev) = seen_iso.insert(iso_lower.clone(), mod_name.clone()) {
            fail(format!(
                "duplicate ISO code '{iso_lower}': declared by both `{prev}` and `{mod_name}`"
            ));
        }

        langs.push(LangInfo {
            mod_name,
            struct_name: local.struct_name,
            morphology_name,
            extra_fields_name: extra_fields,
            grammatical_function_name,
            iso_lower,
        });
    }

    langs.sort_by(|a, b| a.mod_name.cmp(&b.mod_name));

    for l in &langs {
        let tree = trees_dir.join(format!("{}_tree.yaml", l.iso_lower));
        if !tree.exists() {
            fail(format!(
                "missing skill-tree file for language '{}': expected {}",
                l.iso_lower,
                tree.display()
            ));
        }
    }

    let out_dir = std::env::var("OUT_DIR").unwrap();
    let manifest_str = manifest_dir.display().to_string();
    let dest = Path::new(&out_dir).join("lang_registry.rs");

    let mut code = String::new();
    code.push_str("// Auto-generated by langs/build.rs — do not edit\n\n");

    for l in &langs {
        writeln!(
            code,
            "#[path = \"{manifest_str}/src/{mod}.rs\"]\npub mod {mod};",
            mod = l.mod_name,
        )
        .unwrap();
    }
    code.push('\n');

    for l in &langs {
        let extra = l
            .extra_fields_name
            .as_ref()
            .map(|n| format!(", {n}"))
            .unwrap_or_default();
        let gf = l
            .grammatical_function_name
            .as_ref()
            .map(|n| format!(", {n}"))
            .unwrap_or_default();
        writeln!(
            code,
            "pub use {}::{{{}, {}{extra}{gf}}};",
            l.mod_name, l.struct_name, l.morphology_name,
        )
        .unwrap();
    }
    code.push('\n');

    let iso_list = langs
        .iter()
        .map(|l| format!("\"{}\"", l.iso_lower))
        .collect::<Vec<_>>()
        .join(", ");
    writeln!(code, "pub const ALL_ISO_CODES: &[&str] = &[{iso_list}];\n").unwrap();

    code.push_str("#[macro_export]\nmacro_rules! dispatch_iso {\n");
    code.push_str("    ($iso:expr, $lang:ident => $body:expr) => {\n");
    code.push_str("        match $iso {\n");
    for l in &langs {
        writeln!(
            code,
            "            \"{}\" => {{ let $lang = $crate::{}; Some($body) }},",
            l.iso_lower, l.struct_name,
        )
        .unwrap();
    }
    code.push_str("            _ => None,\n");
    code.push_str("        }\n");
    code.push_str("    };\n");
    code.push_str("}\n");

    fs::write(&dest, code)
        .unwrap_or_else(|e| fail(format!("failed to write lang_registry.rs: {e}")));
}
