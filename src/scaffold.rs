//! Rust project generator.
//!
//! Produces a Cargo project that compiles on day one:
//! * `src/models.rs`  - every class/struct/interface/enum as serde structs, enums and traits
//! * `src/ported/**`  - one module per source file, mirroring the original tree, with
//!   typed function/method stubs (`todo!()`) and the original code embedded as
//!   comments next to each stub
//! * `src/routes.rs`  - an axum router with every discovered HTTP route answering 501
//! * `MIGRATION.md`, `ANALYSIS.md`, `c2r-report.json`, `c2r-manifest.json`, `prompts/`

use crate::mapping::{self, Known, TypeNeeds};
use crate::model::{Language, Report, SourceFile, Symbol, SymbolKind};
use crate::plan::Plan;
use crate::scanner::Analysis;
use crate::{prompts, report};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

pub struct Options {
    pub out_dir: PathBuf,
    pub name: Option<String>,
    pub with_deps: bool,
    pub embed_source: bool,
    pub force: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ModuleEntry {
    pub source: String,
    pub language: Language,
    pub rust_file: String,
    pub module_path: String,
    pub prompt: String,
    pub phase: usize,
    pub stubs: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub tool_version: String,
    pub project: String,
    pub crate_name: String,
    pub source_root: String,
    pub web: bool,
    pub models_file: Option<String>,
    pub modules: Vec<ModuleEntry>,
}

pub struct Outcome {
    pub crate_name: String,
    pub out_dir: PathBuf,
    pub models: usize,
    pub traits: usize,
    pub modules: usize,
    pub stubs: usize,
    pub routes: usize,
    pub manifest: Manifest,
}

// ---------------------------------------------------------------------------
// Naming
// ---------------------------------------------------------------------------

const RUST_KEYWORDS: &[&str] = &[
    "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn", "for",
    "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return",
    "self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use", "where",
    "while", "async", "await", "dyn", "abstract", "become", "box", "do", "final", "macro",
    "override", "priv", "typeof", "unsized", "virtual", "yield", "try", "gen", "union",
];

/// Names a generated type must not take because the scaffold relies on them.
const RESERVED_TYPES: &[&str] = &[
    "Self",
    "Option",
    "Some",
    "None",
    "Result",
    "Ok",
    "Err",
    "String",
    "Vec",
    "Box",
    "Default",
    "Clone",
    "Copy",
    "Debug",
    "Send",
    "Sync",
    "Sized",
    "Drop",
    "Fn",
    "FnMut",
    "FnOnce",
    "Iterator",
    "From",
    "Into",
    "ToString",
    "Serialize",
    "Deserialize",
    "PartialEq",
    "Eq",
    "Hash",
    "Ord",
    "PartialOrd",
];

pub fn snake(s: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = s.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if c.is_alphanumeric() {
            if c.is_uppercase() {
                let prev = if i > 0 { chars[i - 1] } else { ' ' };
                let next = chars.get(i + 1).copied().unwrap_or(' ');
                if i > 0
                    && (prev.is_lowercase()
                        || prev.is_ascii_digit()
                        || (prev.is_uppercase() && next.is_lowercase()))
                    && !out.ends_with('_')
                {
                    out.push('_');
                }
                out.extend(c.to_lowercase());
            } else {
                out.push(c.to_ascii_lowercase());
            }
        } else if !out.ends_with('_') && !out.is_empty() {
            out.push('_');
        }
    }
    let out = out.trim_matches('_').to_string();
    let out: String = out
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    if out.is_empty() {
        return "unnamed".into();
    }
    let out = if out.starts_with(|c: char| c.is_ascii_digit()) {
        format!("n_{out}")
    } else {
        out
    };
    if RUST_KEYWORDS.contains(&out.as_str()) {
        format!("{out}_")
    } else {
        out
    }
}

pub fn pascal(s: &str) -> String {
    let mut out = String::new();
    let mut upper_next = true;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            if upper_next {
                out.push(c.to_ascii_uppercase());
                upper_next = false;
            } else {
                out.push(c);
            }
        } else {
            upper_next = true;
        }
    }
    if out.is_empty() {
        return "Unnamed".into();
    }
    if out.starts_with(|c: char| c.is_ascii_digit()) {
        out = format!("T{out}");
    }
    out
}

fn module_name(seg: &str) -> String {
    let n = snake(&seg.replace(['.', '-'], "_"));
    let n = n.trim_end_matches('_').to_string();
    match n.as_str() {
        "" => "m".into(),
        "self" | "super" | "crate" | "mod" | "std" | "core" | "alloc" => format!("{n}_mod"),
        x if RUST_KEYWORDS.contains(&x) => format!("{x}_mod"),
        _ => n,
    }
}

fn unique(name: String, taken: &mut BTreeSet<String>) -> String {
    if taken.insert(name.clone()) {
        return name;
    }
    for i in 2.. {
        let cand = format!("{name}_{i}");
        if taken.insert(cand.clone()) {
            return cand;
        }
    }
    unreachable!()
}

fn doc_safe(s: &str) -> String {
    s.replace('`', "'").replace('\r', "")
}

// ---------------------------------------------------------------------------
// Models
// ---------------------------------------------------------------------------

enum ModelKind {
    Struct,
    Enum,
    Trait,
}

struct Model<'a> {
    rust_name: String,
    kind: ModelKind,
    sym: &'a Symbol,
    file: &'a SourceFile,
}

fn is_data_language(l: Language) -> bool {
    matches!(l, Language::TypeScript | Language::JavaScript)
}

fn collect_models<'a>(files: &[&'a SourceFile]) -> (Vec<Model<'a>>, Known, BTreeSet<String>) {
    let mut models = vec![];
    let mut known: Known = BTreeMap::new();
    let mut traits: BTreeSet<String> = BTreeSet::new();
    let mut taken: BTreeSet<String> = BTreeSet::new();
    for f in files {
        for s in &f.symbols {
            let kind = match s.kind {
                SymbolKind::Enum => ModelKind::Enum,
                SymbolKind::Interface if is_data_language(f.language) => ModelKind::Struct,
                SymbolKind::Interface => ModelKind::Trait,
                SymbolKind::Class | SymbolKind::Struct => ModelKind::Struct,
                _ => continue,
            };
            let original = pascal(&s.name);
            if known.contains_key(&original) || traits.contains(&original) {
                continue; // first definition wins (partial classes, re-declarations)
            }
            let mut rust_name = original.clone();
            if RESERVED_TYPES.contains(&rust_name.as_str()) {
                rust_name.push_str("Model");
            }
            let rust_name = {
                let mut n = rust_name;
                while !taken.insert(n.clone()) {
                    n.push('2');
                }
                n
            };
            match kind {
                ModelKind::Trait => {
                    traits.insert(original);
                }
                _ => {
                    known.insert(original, rust_name.clone());
                }
            }
            models.push(Model {
                rust_name,
                kind,
                sym: s,
                file: f,
            });
        }
    }
    (models, known, traits)
}

fn render_models(models: &[Model], known: &Known, needs: &mut TypeNeeds) -> String {
    let mut out = String::new();
    out.push_str("//! Data types recovered from the original project.\n//!\n//! Field names are snake_case; `#[serde(rename)]` keeps the original JSON/wire names.\n//! Unknown or dynamic types are `serde_json::Value` placeholders: tighten them as you port.\n\nuse serde::{Deserialize, Serialize};\n\n");
    let struct_names: BTreeSet<&str> = models
        .iter()
        .filter(|m| matches!(m.kind, ModelKind::Struct))
        .map(|m| m.rust_name.as_str())
        .collect();

    // Map every field type once, then find which direct struct-to-struct
    // edges form cycles: only those need `Box` to give the type a finite size.
    let mut field_types: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for m in models
        .iter()
        .filter(|m| matches!(m.kind, ModelKind::Struct))
    {
        let tys = m
            .sym
            .fields
            .iter()
            .map(|f| {
                let mut ty = mapping::rust_type(m.file.language, f.ty.as_deref(), known, needs);
                if ty.contains("anyhow::Error") {
                    ty = ty.replace("anyhow::Error", "String");
                }
                if ty == "()" {
                    ty = "serde_json::Value".into();
                }
                ty
            })
            .collect();
        field_types.insert(m.rust_name.clone(), tys);
    }
    let bare = |ty: &str| {
        ty.strip_prefix("Option<")
            .and_then(|t| t.strip_suffix('>'))
            .unwrap_or(ty)
            .to_string()
    };
    let mut edges: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for m in models
        .iter()
        .filter(|m| matches!(m.kind, ModelKind::Struct))
    {
        let e = edges.entry(m.rust_name.clone()).or_default();
        for ty in &field_types[&m.rust_name] {
            let b = bare(ty);
            if struct_names.contains(b.as_str()) {
                e.insert(b);
            }
        }
        for base in &m.sym.bases {
            if let Some(rust) = known.get(&pascal(base.split('<').next().unwrap_or(base).trim())) {
                if struct_names.contains(rust.as_str()) {
                    e.insert(rust.clone());
                }
            }
        }
    }
    let reaches = |from: &str, to: &str| -> bool {
        let mut stack = vec![from.to_string()];
        let mut seen = BTreeSet::new();
        while let Some(n) = stack.pop() {
            if n == to {
                return true;
            }
            if seen.insert(n.clone()) {
                if let Some(next) = edges.get(&n) {
                    stack.extend(next.iter().cloned());
                }
            }
        }
        false
    };

    for m in models {
        let s = m.sym;
        let _ = writeln!(
            out,
            "/// Ported from `{}:{}` ({} {}).",
            m.file.path,
            s.line,
            m.file.language.name(),
            kind_label(s.kind)
        );
        if !s.bases.is_empty() {
            let _ = writeln!(
                out,
                "/// Original bases: {}.",
                doc_safe(&s.bases.join(", "))
            );
        }
        match m.kind {
            ModelKind::Enum => {
                out.push_str(
                    "#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]\n",
                );
                let _ = writeln!(out, "pub enum {} {{", m.rust_name);
                let mut taken = BTreeSet::new();
                let mut any = false;
                for v in &s.fields {
                    let name = pascal(&v.name.to_ascii_lowercase());
                    let name = if RESERVED_TYPES.contains(&name.as_str()) {
                        format!("{name}Variant")
                    } else {
                        name
                    };
                    if !taken.insert(name.clone()) {
                        continue;
                    }
                    // `ty` carries the enum's wire value when the source declared one.
                    let wire = v.ty.clone().unwrap_or_else(|| v.name.clone());
                    if name != wire {
                        let _ =
                            writeln!(out, "    #[serde(rename = \"{}\")]", wire.replace('"', ""));
                    }
                    let _ = writeln!(out, "    {name},");
                    any = true;
                }
                if !any {
                    out.push_str("    /// c2r: variants were not recovered; fill them in from the original.\n    Unknown,\n");
                }
                out.push_str("}\n\n");
            }
            ModelKind::Trait => {
                let _ = writeln!(out, "/// Interface -> trait. Add the method signatures as you port the implementors.");
                let _ = writeln!(out, "pub trait {} {{}}\n", m.rust_name);
            }
            ModelKind::Struct => {
                out.push_str("#[derive(Debug, Clone, Serialize, Deserialize)]\n");
                let _ = writeln!(out, "pub struct {} {{", m.rust_name);
                let mut taken = BTreeSet::new();
                // Composition replaces inheritance from a known base struct.
                for b in &s.bases {
                    let base = b.split('<').next().unwrap_or(b).trim();
                    if let Some(rust) = known.get(&pascal(base)) {
                        if rust != &m.rust_name && struct_names.contains(rust.as_str()) {
                            let fname = unique(snake(base), &mut taken);
                            let ty = if reaches(rust, &m.rust_name) {
                                format!("Box<{rust}>")
                            } else {
                                rust.clone()
                            };
                            let _ = writeln!(out, "    /// Inherited fields from `{base}` (composition replaces inheritance).\n    #[serde(flatten)]\n    pub {fname}: {ty},");
                        }
                    }
                }
                for (f, mapped) in s.fields.iter().zip(field_types[&m.rust_name].iter()) {
                    let mut ty = mapped.clone();
                    let b = bare(&ty);
                    let was_option = ty.starts_with("Option<");
                    if struct_names.contains(b.as_str()) && reaches(&b, &m.rust_name) {
                        ty = if was_option || f.optional {
                            format!("Option<Box<{b}>>")
                        } else {
                            format!("Box<{b}>")
                        };
                    } else if f.optional && !was_option {
                        ty = format!("Option<{ty}>");
                    }
                    let name = unique(snake(&f.name), &mut taken);
                    if ty.contains("serde_json::Value") {
                        if let Some(orig) = f.ty.as_deref().filter(|o| {
                            !matches!(
                                o.trim(),
                                "any"
                                    | "Any"
                                    | "object"
                                    | "Object"
                                    | "unknown"
                                    | "interface{}"
                                    | "dynamic"
                                    | "mixed"
                            )
                        }) {
                            let _ = writeln!(
                                out,
                                "    /// c2r: original type `{}`; replace the placeholder.",
                                doc_safe(orig.trim())
                            );
                        }
                    }
                    if name != f.name {
                        let _ = writeln!(
                            out,
                            "    #[serde(rename = \"{}\")]",
                            f.name.replace('"', "")
                        );
                    }
                    if ty.starts_with("Option<") {
                        out.push_str("    #[serde(default)]\n");
                    }
                    let _ = writeln!(out, "    pub {name}: {ty},");
                }
                out.push_str("}\n\n");
            }
        }
    }
    out
}

fn kind_label(k: SymbolKind) -> &'static str {
    match k {
        SymbolKind::Class => "class",
        SymbolKind::Struct => "struct",
        SymbolKind::Interface => "interface",
        SymbolKind::Enum => "enum",
        SymbolKind::Function => "function",
        SymbolKind::Method => "method",
    }
}

// ---------------------------------------------------------------------------
// Functions
// ---------------------------------------------------------------------------

const CONSTRUCTORS: &[&str] = &[
    "__init__",
    "constructor",
    "initialize",
    "new",
    "__construct",
    "init",
];

struct FnCtx<'a> {
    file: &'a SourceFile,
    source: &'a str,
    known: &'a Known,
    embed: bool,
}

fn returns_value(body: &str) -> bool {
    body.lines().skip(1).any(|l| {
        let t = l.trim();
        (t.starts_with("return ") && t.trim_end_matches(';') != "return") || t.starts_with("yield ")
    })
}

fn is_static(ctx: &FnCtx, s: &Symbol) -> bool {
    if s.signature.split_whitespace().any(|w| w == "static") {
        return true;
    }
    if ctx.file.language == Language::Python && s.line >= 2 {
        let prev = ctx.source.lines().nth(s.line - 2).unwrap_or("").trim();
        return prev.starts_with("@staticmethod") || prev.starts_with("@classmethod");
    }
    false
}

fn render_fn(
    ctx: &FnCtx,
    s: &Symbol,
    receiver: Option<&str>,
    needs: &mut TypeNeeds,
    name_taken: &mut BTreeSet<String>,
    indent: &str,
) -> String {
    let lang = ctx.file.language;
    let is_ctor = receiver.is_some()
        && (CONSTRUCTORS.contains(&s.name.as_str())
            || s.parent.as_deref() == Some(s.name.as_str()));
    let fn_name = if is_ctor {
        unique("new".into(), name_taken)
    } else {
        let n = match s.name.as_str() {
            "__str__" | "__repr__" | "toString" | "ToString" | "to_s" | "String" => {
                "to_display_string".to_string()
            }
            "__eq__" | "equals" | "Equals" => "equals".to_string(),
            "__hash__" | "hashCode" | "GetHashCode" => "hash_code".to_string(),
            other => snake(other.trim_matches('_').trim_end_matches(['?', '!', '='])),
        };
        unique(n, name_taken)
    };

    let mut params: Vec<String> = vec![];
    match receiver {
        Some(_) if is_ctor || is_static(ctx, s) => {}
        Some(_) => params.push("&self".into()),
        None => {}
    }
    let mut ptaken: BTreeSet<String> = ["self".to_string()].into_iter().collect();
    for p in &s.params {
        let pname = unique(snake(&p.name), &mut ptaken);
        let pname = if pname == "self_" {
            unique("this".into(), &mut ptaken)
        } else {
            pname
        };
        let mut ty = mapping::rust_type(lang, p.ty.as_deref(), ctx.known, needs);
        if ty == "()" {
            ty = "serde_json::Value".into();
        }
        params.push(format!("{pname}: {ty}"));
    }

    let body_text = crate::text::slice_lines(ctx.source, s.line, s.end_line.max(s.line));
    let ret = if is_ctor {
        Some("Self".to_string())
    } else {
        match &s.return_type {
            Some(r) if lang == Language::Go && r.trim() == "error" => {
                Some("anyhow::Result<()>".to_string())
            }
            Some(r) => {
                Some(mapping::rust_type(lang, Some(r), ctx.known, needs)).filter(|t| t != "()")
            }
            None if matches!(
                lang,
                Language::Python
                    | Language::JavaScript
                    | Language::TypeScript
                    | Language::Ruby
                    | Language::Php
            ) =>
            {
                returns_value(&body_text).then(|| "serde_json::Value".to_string())
            }
            None => None,
        }
    };

    let mut out = String::new();
    let _ = writeln!(
        out,
        "{indent}/// Ported from `{}:{}`.",
        ctx.file.path, s.line
    );
    let _ = writeln!(
        out,
        "{indent}/// Original: `{}`",
        doc_safe(s.signature.trim_end_matches('{').trim())
    );
    let asyncness = if s.is_async { "async " } else { "" };
    let ret_s = ret.map(|r| format!(" -> {r}")).unwrap_or_default();
    let _ = writeln!(
        out,
        "{indent}pub {asyncness}fn {fn_name}({}){ret_s} {{",
        params.join(", ")
    );
    if ctx.embed {
        let lines: Vec<&str> = body_text.lines().collect();
        if !lines.is_empty() {
            let _ = writeln!(out, "{indent}    // ---- original ({}) ----", lang.name());
            for l in lines.iter().take(80) {
                let _ = writeln!(
                    out,
                    "{indent}    // {}",
                    l.replace('\r', "").replace('\t', "    ")
                );
            }
            if lines.len() > 80 {
                let _ = writeln!(
                    out,
                    "{indent}    // ... {} more lines in {}",
                    lines.len() - 80,
                    ctx.file.path
                );
            }
            let _ = writeln!(out, "{indent}    // ---- end original ----");
        }
    }
    let _ = writeln!(
        out,
        "{indent}    todo!(\"port `{}` from {}:{}\")",
        s.name.replace('"', ""),
        ctx.file.path,
        s.line
    );
    let _ = writeln!(out, "{indent}}}");
    out
}

/// Render one source file as a Rust module body. Returns (code, stub count).
fn render_module(
    ctx: &FnCtx,
    has_models: bool,
    struct_names: &BTreeSet<String>,
    impl_taken: &mut BTreeMap<String, BTreeSet<String>>,
    needs: &mut TypeNeeds,
) -> (String, usize) {
    let f = ctx.file;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "//! Ported from `{}` ({}, {} lines).",
        f.path,
        f.language.name(),
        f.lines
    );
    out.push_str("//!\n//! c2r generated typed stubs. Replace each `todo!()` with the real logic;\n//! the original code is embedded as comments inside every stub.\n\n");
    if has_models {
        out.push_str("use crate::models::*;\n\n");
    }
    let mut stubs = 0;
    let mut free_taken: BTreeSet<String> = BTreeSet::new();

    // Free functions (plus methods whose parent is not a generated struct).
    for s in f
        .symbols
        .iter()
        .filter(|s| matches!(s.kind, SymbolKind::Function | SymbolKind::Method))
    {
        let parent_struct = s
            .parent
            .as_ref()
            .and_then(|p| ctx.known.get(&pascal(p)))
            .filter(|r| struct_names.contains(*r));
        if parent_struct.is_some() {
            continue;
        }
        let mut s2 = s.clone();
        if let Some(p) = &s.parent {
            // e.g. Kotlin extension functions, Ruby modules, methods of interfaces.
            s2.name = format!("{}_{}", snake(p), s.name);
        }
        out.push_str(&render_fn(ctx, &s2, None, needs, &mut free_taken, ""));
        out.push('\n');
        stubs += 1;
    }

    // Methods grouped into impl blocks.
    let mut by_parent: BTreeMap<String, Vec<&Symbol>> = BTreeMap::new();
    for s in f.symbols.iter().filter(|s| s.kind == SymbolKind::Method) {
        if let Some(rust) = s
            .parent
            .as_ref()
            .and_then(|p| ctx.known.get(&pascal(p)))
            .filter(|r| struct_names.contains(*r))
        {
            by_parent.entry(rust.clone()).or_default().push(s);
        }
    }
    for (rust, methods) in by_parent {
        let _ = writeln!(out, "impl {rust} {{");
        let taken = impl_taken.entry(rust.clone()).or_default();
        for s in methods {
            out.push_str(&render_fn(ctx, s, Some(&rust), needs, taken, "    "));
            out.push('\n');
            stubs += 1;
        }
        if out.ends_with("\n\n") {
            out.pop();
        }
        out.push_str("}\n\n");
    }
    (out.trim_end().to_string() + "\n", stubs)
}

// ---------------------------------------------------------------------------
// Module tree
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Node {
    content: Option<String>,
    source: Option<String>,
    children: BTreeMap<String, Node>,
}

fn common_dir_prefix(paths: &[&str]) -> Vec<String> {
    let split: Vec<Vec<&str>> = paths
        .iter()
        .map(|p| p.split('/').collect::<Vec<_>>())
        .collect();
    let Some(first) = split.first() else {
        return vec![];
    };
    let mut prefix = vec![];
    for (i, seg) in first.iter().enumerate().take(first.len().saturating_sub(1)) {
        if split.iter().all(|s| s.len() > i + 1 && s[i] == *seg) {
            prefix.push(seg.to_string());
        } else {
            break;
        }
    }
    prefix
}

fn module_segments(path: &str, strip: usize) -> Vec<String> {
    let parts: Vec<&str> = path.split('/').skip(strip).collect();
    let mut segs: Vec<String> = vec![];
    for (i, p) in parts.iter().enumerate() {
        let is_last = i + 1 == parts.len();
        let stem = if is_last {
            p.rsplit_once('.').map(|(a, _)| a).unwrap_or(p)
        } else {
            p
        };
        if is_last && matches!(stem, "__init__" | "index" | "mod") && !segs.is_empty() {
            continue; // package file -> content of the directory module
        }
        segs.push(module_name(stem));
    }
    if segs.is_empty() {
        segs.push("root".into());
    }
    segs
}

fn write_tree(
    node: &Node,
    dir: &Path,
    name: &str,
    module_path: &str,
    files: &mut BTreeMap<String, (String, String)>,
    base: &Path,
) -> Result<()> {
    let (file, child_dir) = if node.children.is_empty() {
        (dir.join(format!("{name}.rs")), None)
    } else {
        (dir.join(name).join("mod.rs"), Some(dir.join(name)))
    };
    if let Some(p) = file.parent() {
        fs::create_dir_all(p)?;
    }
    let mut body = node
        .content
        .clone()
        .unwrap_or_else(|| "//! Module namespace generated by c2r.\n".to_string());
    if !node.children.is_empty() {
        body.push('\n');
        for child in node.children.keys() {
            let _ = writeln!(body, "pub mod {child};");
        }
    }
    fs::write(&file, body)?;
    if let Some(src) = &node.source {
        let rel = file
            .strip_prefix(base)
            .unwrap_or(&file)
            .to_string_lossy()
            .replace('\\', "/");
        files.insert(src.clone(), (rel, module_path.to_string()));
    }
    if let Some(cd) = child_dir {
        for (cname, child) in &node.children {
            write_tree(
                child,
                &cd,
                cname,
                &format!("{module_path}::{cname}"),
                files,
                base,
            )?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Routes
// ---------------------------------------------------------------------------

pub fn axum_path(raw: &str) -> String {
    let raw = raw.split('?').next().unwrap_or(raw).trim();
    let raw = raw.trim_start_matches('^').trim_end_matches('$');
    let mut segs: Vec<String> = vec![];
    let mut used: BTreeSet<String> = BTreeSet::new();
    let param = |name: &str, used: &mut BTreeSet<String>| {
        let n = snake(
            name.trim_end_matches('?')
                .trim_end_matches('*')
                .trim_end_matches('+'),
        );
        let n = if n == "unnamed" {
            "param".to_string()
        } else {
            n.trim_end_matches('_').to_string()
        };
        unique(n, used)
    };
    let parts: Vec<&str> = raw.split('/').collect();
    for (i, seg) in parts.iter().enumerate() {
        let is_last = i + 1 == parts.len();
        if seg.is_empty() {
            continue;
        }
        let s = *seg;
        let converted = if let Some(rest) = s.strip_prefix(':') {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            format!("{{{}}}", param(&name, &mut used))
        } else if s.starts_with('<') && s.ends_with('>') {
            let inner = &s[1..s.len() - 1];
            let name = inner.rsplit(':').next().unwrap_or(inner);
            if inner.starts_with("path:") && is_last {
                format!("{{*{}}}", param(name, &mut used))
            } else {
                format!("{{{}}}", param(name, &mut used))
            }
        } else if s.starts_with('{') && s.ends_with('}') {
            let inner = s[1..s.len() - 1].trim_start_matches('*');
            let (name, conv) = inner.split_once(':').unwrap_or((inner, ""));
            if (conv == "path" || s.starts_with("{*") || conv.contains(".*")) && is_last {
                format!("{{*{}}}", param(name, &mut used))
            } else {
                format!("{{{}}}", param(name, &mut used))
            }
        } else if s.starts_with('*') {
            if !is_last {
                break;
            }
            let name = s.trim_start_matches('*');
            format!(
                "{{*{}}}",
                param(if name.is_empty() { "rest" } else { name }, &mut used)
            )
        } else if let Some(idx) = s.find("(?P<") {
            let after = &s[idx + 4..];
            let name = after.split('>').next().unwrap_or("param");
            format!("{{{}}}", param(name, &mut used))
        } else if s == "[controller]" || s == "[action]" {
            s.trim_matches(|c| c == '[' || c == ']').to_string()
        } else {
            s.chars()
                .filter(|c| {
                    !matches!(
                        c,
                        '{' | '}'
                            | '('
                            | ')'
                            | '^'
                            | '$'
                            | '\\'
                            | '?'
                            | '*'
                            | '['
                            | ']'
                            | '|'
                            | '+'
                    )
                })
                .collect()
        };
        if !converted.is_empty() {
            segs.push(converted);
        }
    }
    let mut p = format!("/{}", segs.join("/"));
    if raw.ends_with('/') && p != "/" {
        p.push('/');
    }
    p
}

fn shape(path: &str) -> String {
    path.split('/')
        .map(|s| {
            if s.starts_with("{*") {
                "{*}"
            } else if s.starts_with('{') {
                "{}"
            } else {
                s
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

struct RouteGen {
    path: String,
    method: String,
    handler_fn: String,
    origin: String,
    original: String,
    target: Option<String>,
}

fn render_routes(
    report: &Report,
    module_paths: &BTreeMap<String, (String, String)>,
) -> (String, usize) {
    let mut by_path: BTreeMap<String, Vec<RouteGen>> = BTreeMap::new();
    let mut shapes: BTreeMap<String, String> = BTreeMap::new();
    // Names already used by routes.rs itself (imports and helpers).
    const RESERVED_HANDLERS: &[&str] = &[
        "router",
        "not_found",
        "get",
        "post",
        "put",
        "delete",
        "patch",
        "head",
        "options",
        "any",
        "json",
        "path",
        "status_code",
    ];
    let mut taken: BTreeSet<String> = RESERVED_HANDLERS.iter().map(|s| s.to_string()).collect();
    let mut dropped: Vec<String> = vec![];
    for (f, r) in report.all_routes() {
        if f.is_test {
            continue;
        }
        let mut path = axum_path(&r.path);
        // Same shape, different param names would make axum panic: reuse the first.
        let sh = shape(&path);
        match shapes.get(&sh) {
            Some(existing) => path = existing.clone(),
            None => {
                shapes.insert(sh, path.clone());
            }
        }
        let method = match r.method.as_str() {
            "GET" | "POST" | "PUT" | "DELETE" | "PATCH" | "HEAD" | "OPTIONS" => {
                r.method.to_ascii_lowercase()
            }
            _ => "any".to_string(),
        };
        let entry = by_path.entry(path.clone()).or_default();
        if entry
            .iter()
            .any(|g| g.method == method || g.method == "any" || method == "any")
        {
            dropped.push(format!("{} {} ({}:{})", r.method, r.path, f.path, r.line));
            continue;
        }
        let fallback = || {
            let p = path.replace(['{', '}', '*'], "");
            let p = p.trim_matches('/');
            format!("{}_{}", method, if p.is_empty() { "index" } else { p })
        };
        let base = r
            .handler
            .clone()
            .filter(|h| !RESERVED_HANDLERS.contains(&snake(h).as_str()))
            .unwrap_or_else(fallback);
        let handler_fn = unique(snake(&base), &mut taken);
        let target = r.handler.as_ref().and_then(|h| {
            let has = f.symbols.iter().any(|s| {
                &s.name == h
                    && matches!(s.kind, SymbolKind::Function | SymbolKind::Method)
                    && s.parent.is_none()
            });
            let (_, module) = module_paths.get(&f.path)?;
            has.then(|| format!("{module}::{}", snake(h)))
        });
        entry.push(RouteGen {
            path: path.clone(),
            method,
            handler_fn,
            origin: format!("{}:{}", f.path, r.line),
            original: format!("{} {} ({})", r.method, r.path, r.framework),
            target,
        });
    }

    let mut out = String::new();
    out.push_str("//! HTTP routes recovered from the original project.\n//!\n//! Every handler currently answers `501 Not Implemented` with a JSON body naming\n//! the original route, so the service runs end-to-end from day one. Port each\n//! handler, then call into `crate::ported::...` for the business logic.\n\n");
    out.push_str("use axum::{\n    extract::Path,\n    http::StatusCode,\n    response::IntoResponse,\n    routing::{any, delete, get, head, options, patch, post, put},\n    Json, Router,\n};\nuse serde_json::json;\nuse std::collections::HashMap;\n\n");
    out.push_str("pub fn router() -> Router {\n    Router::new()\n");
    let mut count = 0;
    for (path, gens) in &by_path {
        let chain = gens
            .iter()
            .map(|g| format!("{}({})", g.method, g.handler_fn))
            .collect::<Vec<_>>()
            .join(".");
        let _ = writeln!(out, "        .route(\"{path}\", {chain})");
        count += gens.len();
    }
    out.push_str("        .fallback(not_found)\n}\n\n");
    for gens in by_path.values() {
        for g in gens {
            let _ = writeln!(
                out,
                "/// `{}` - ported from `{}`.",
                doc_safe(&g.original),
                g.origin
            );
            if let Some(t) = &g.target {
                let _ = writeln!(out, "/// Business logic stub: `{t}`.");
            }
            let has_params = g.path.contains('{');
            let args = if has_params {
                "Path(params): Path<HashMap<String, String>>"
            } else {
                ""
            };
            let _ = writeln!(
                out,
                "async fn {}({args}) -> impl IntoResponse {{",
                g.handler_fn
            );
            let params = if has_params {
                "params"
            } else {
                "HashMap::<String, String>::new()"
            };
            let _ = writeln!(
                out,
                "    (\n        StatusCode::NOT_IMPLEMENTED,\n        Json(json!({{\n            \"error\": \"not implemented yet\",\n            \"route\": \"{} {}\",\n            \"ported_from\": \"{}\",\n            \"params\": {params},\n        }})),\n    )\n}}\n",
                g.method.to_ascii_uppercase(),
                g.path,
                g.origin
            );
        }
    }
    out.push_str("async fn not_found() -> impl IntoResponse {\n    (StatusCode::NOT_FOUND, Json(json!({ \"error\": \"no such route\" })))\n}\n");
    if !dropped.is_empty() {
        out.push_str(
            "\n// c2r: these routes overlapped an existing method+path and were not registered:\n",
        );
        for d in dropped {
            let _ = writeln!(out, "//   {d}");
        }
    }
    // Silence unused-import warnings for verbs that are not used.
    out = out.replacen("use axum::{", "#[allow(unused_imports)]\nuse axum::{", 1);
    (out, count)
}

// ---------------------------------------------------------------------------
// Cargo.toml
// ---------------------------------------------------------------------------

fn dep_line(name: &str, version: &str, features: &BTreeSet<String>) -> String {
    if features.is_empty() {
        format!("{name} = \"{version}\"")
    } else {
        let f = features
            .iter()
            .map(|x| format!("\"{x}\""))
            .collect::<Vec<_>>()
            .join(", ");
        format!("{name} = {{ version = \"{version}\", features = [{f}] }}")
    }
}

fn crate_name(report: &Report, override_name: Option<&str>) -> String {
    let base = override_name
        .map(str::to_string)
        .unwrap_or_else(|| format!("{}-rs", report.project_name));
    let n: String = base
        .to_ascii_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let n = n.trim_matches('-').to_string();
    let n = if n.is_empty() || n.starts_with(|c: char| c.is_ascii_digit()) {
        format!("app-{n}")
    } else {
        n
    };
    if matches!(
        n.as_str(),
        "test" | "core" | "std" | "alloc" | "proc-macro" | "self"
    ) {
        format!("{n}-rs")
    } else {
        n
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

pub fn generate(analysis: &Analysis, plan: &Plan, opts: &Options) -> Result<Outcome> {
    let report = &analysis.report;
    let out = &opts.out_dir;
    if out.exists() && fs::read_dir(out)?.next().is_some() && !opts.force {
        bail!(
            "output directory '{}' is not empty (use --force to write into it)",
            out.display()
        );
    }
    fs::create_dir_all(out.join("src"))
        .with_context(|| format!("cannot create {}", out.display()))?;
    let crate_name = crate_name(report, opts.name.as_deref());
    let web = mapping::is_web_project(report);

    let portable: Vec<&SourceFile> = report
        .files
        .iter()
        .filter(|f| f.language.is_portable() && !f.is_test)
        .collect();
    let (models, known, traits) = collect_models(&portable);
    let mut needs = TypeNeeds::default();
    let struct_names: BTreeSet<String> = models
        .iter()
        .filter(|m| !matches!(m.kind, ModelKind::Trait))
        .map(|m| m.rust_name.clone())
        .collect();
    let has_models = !models.is_empty();
    if has_models {
        let code = render_models(&models, &known, &mut needs);
        fs::write(out.join("src/models.rs"), code)?;
    }

    // Module tree.
    let module_files: Vec<&SourceFile> = portable
        .iter()
        .copied()
        .filter(|f| {
            f.symbols
                .iter()
                .any(|s| matches!(s.kind, SymbolKind::Function | SymbolKind::Method))
        })
        .collect();
    let paths: Vec<&str> = module_files.iter().map(|f| f.path.as_str()).collect();
    let strip = if paths.len() > 1 {
        common_dir_prefix(&paths).len()
    } else {
        paths.first().map(|p| p.matches('/').count()).unwrap_or(0)
    };
    let mut root = Node::default();
    let mut impl_taken: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut stub_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut used_paths: BTreeSet<Vec<String>> = BTreeSet::new();
    for f in &module_files {
        let source = analysis
            .read_source(&f.path)
            .unwrap_or_default()
            .replace("\r\n", "\n");
        let ctx = FnCtx {
            file: f,
            source: &source,
            known: &known,
            embed: opts.embed_source,
        };
        let (code, stubs) =
            render_module(&ctx, has_models, &struct_names, &mut impl_taken, &mut needs);
        let mut segs = module_segments(&f.path, strip);
        while used_paths.contains(&segs) {
            let last = segs.pop().unwrap();
            segs.push(format!("{last}_2"));
        }
        used_paths.insert(segs.clone());
        let mut node = &mut root;
        for s in &segs {
            node = node.children.entry(s.clone()).or_default();
        }
        node.content = Some(match node.content.take() {
            Some(existing) => format!("{existing}\n{code}"),
            None => code,
        });
        node.source = Some(f.path.clone());
        stub_counts.insert(f.path.clone(), stubs);
    }
    let mut module_paths: BTreeMap<String, (String, String)> = BTreeMap::new();
    let has_modules = !module_files.is_empty();
    if has_modules {
        root.content = Some("//! Ported modules, mirroring the original source tree.\n".into());
        let src = out.join("src");
        write_tree(
            &root,
            &src,
            "ported",
            "crate::ported",
            &mut module_paths,
            out,
        )?;
    }

    // Routes.
    let (routes_code, route_count) = render_routes(report, &module_paths);
    if web {
        fs::write(out.join("src/routes.rs"), routes_code)?;
    }

    // main.rs
    let mut main = String::new();
    let _ = writeln!(
        main,
        "//! {crate_name}: Rust rebuild of `{}`, scaffolded by convert-to-rust (c2r).",
        report.project_name
    );
    main.push_str("//!\n//! See MIGRATION.md for the phase-by-phase porting plan.\n\n");
    main.push_str("// Generated stubs are intentionally unused until ported.\n#![allow(dead_code, unused_variables, unused_imports, clippy::all)]\n\n");
    if has_models {
        main.push_str("mod models;\n");
    }
    if has_modules {
        main.push_str("mod ported;\n");
    }
    if web {
        main.push_str("mod routes;\n\n");
        main.push_str("#[tokio::main]\nasync fn main() -> anyhow::Result<()> {\n    tracing_subscriber::fmt()\n        .with_env_filter(std::env::var(\"RUST_LOG\").unwrap_or_else(|_| \"info\".into()))\n        .init();\n    let port = std::env::var(\"PORT\").unwrap_or_else(|_| \"3000\".into());\n    let addr = format!(\"0.0.0.0:{port}\");\n    let listener = tokio::net::TcpListener::bind(&addr).await?;\n    tracing::info!(\"listening on http://{addr}\");\n    axum::serve(listener, routes::router()).await?;\n    Ok(())\n}\n");
    } else {
        main.push('\n');
        main.push_str("fn main() -> anyhow::Result<()> {\n");
        if report.entry_points.is_empty() {
            let _ = writeln!(main, "    println!(\"{crate_name}: scaffold ready. Port the stubs in src/ and wire your entry point here.\");");
        } else {
            let _ = writeln!(
                main,
                "    // Original entry point(s): {}",
                report.entry_points.join(", ")
            );
            let _ = writeln!(main, "    println!(\"{crate_name}: scaffold ready. Port the original entry point(s) ({}) into main().\");", report.entry_points.join(", ").replace('"', ""));
        }
        main.push_str("    Ok(())\n}\n");
    }
    fs::write(out.join("src/main.rs"), main)?;

    // Cargo.toml
    let mut active: BTreeMap<String, (String, BTreeSet<String>)> = BTreeMap::new();
    let mut add = |n: &str, v: &str, f: &[&str]| {
        let e = active
            .entry(n.to_string())
            .or_insert_with(|| (v.to_string(), BTreeSet::new()));
        e.1.extend(f.iter().map(|x| x.to_string()));
    };
    add("anyhow", "1", &[]);
    add("serde", "1", &["derive"]);
    add("serde_json", "1", &[]);
    if web {
        add("axum", "0.8", &[]);
        add("tokio", "1", &["full"]);
        add("tracing", "0.1", &[]);
        add("tracing-subscriber", "0.3", &["env-filter"]);
    }
    if needs.chrono {
        add("chrono", "0.4", &["serde"]);
    }
    if needs.uuid {
        add("uuid", "1", &["v4", "serde"]);
    }
    if needs.decimal {
        add("rust_decimal", "1", &["serde"]);
    }
    let suggested = mapping::suggested_crates(report);
    let mut commented: Vec<String> = vec![];
    for (name, (ver, feats)) in &suggested {
        if opts.with_deps {
            let e = active
                .entry(name.clone())
                .or_insert_with(|| (ver.clone(), BTreeSet::new()));
            e.1.extend(feats.iter().cloned());
        } else if !active.contains_key(name) {
            commented.push(format!("# {}", dep_line(name, ver, feats)));
        }
    }
    let mut cargo = String::new();
    let _ = writeln!(cargo, "[package]\nname = \"{crate_name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\ndescription = \"Rust rebuild of {} (scaffolded by convert-to-rust)\"\npublish = false\n", report.project_name.replace('"', ""));
    cargo.push_str("# Standalone project, even when generated inside another Cargo workspace.\n[workspace]\n\n[dependencies]\n");
    for (name, (ver, feats)) in &active {
        let _ = writeln!(cargo, "{}", dep_line(name, ver, feats));
    }
    if !commented.is_empty() {
        cargo.push_str(
            "\n# --- Suggested by c2r from the original dependencies: uncomment as you port ---\n",
        );
        for c in commented {
            let _ = writeln!(cargo, "{c}");
        }
    }
    fs::write(out.join("Cargo.toml"), cargo)?;
    fs::write(out.join(".gitignore"), "/target\n")?;

    // Manifest + prompts + docs.
    let phase_of: BTreeMap<String, usize> = plan
        .phases
        .iter()
        .flat_map(|p| p.modules.iter().map(move |m| (m.path.clone(), p.number)))
        .collect();
    let mut entries: Vec<ModuleEntry> = module_files
        .iter()
        .filter_map(|f| {
            let (rust_file, module_path) = module_paths.get(&f.path)?.clone();
            let phase = phase_of.get(&f.path).copied().unwrap_or(1);
            Some(ModuleEntry {
                source: f.path.clone(),
                language: f.language,
                rust_file,
                module_path,
                prompt: String::new(),
                phase,
                stubs: stub_counts.get(&f.path).copied().unwrap_or(0),
            })
        })
        .collect();
    entries.sort_by(|a, b| (a.phase, &a.source).cmp(&(b.phase, &b.source)));
    for (i, e) in entries.iter_mut().enumerate() {
        e.prompt = format!(
            "prompts/{:03}-{}.md",
            i + 1,
            snake(&e.source.replace('/', "_"))
        );
    }
    let manifest = Manifest {
        tool_version: env!("CARGO_PKG_VERSION").into(),
        project: report.project_name.clone(),
        crate_name: crate_name.clone(),
        source_root: analysis
            .root
            .canonicalize()
            .unwrap_or_else(|_| analysis.root.clone())
            .to_string_lossy()
            .to_string(),
        web,
        models_file: has_models.then(|| "src/models.rs".into()),
        modules: entries,
    };
    prompts::write_all(analysis, plan, &manifest, out)?;
    fs::write(
        out.join("c2r-manifest.json"),
        serde_json::to_string_pretty(&manifest)?,
    )?;
    fs::write(
        out.join("c2r-report.json"),
        serde_json::to_string_pretty(report)?,
    )?;
    fs::write(out.join("ANALYSIS.md"), report::analysis_markdown(report))?;
    fs::write(
        out.join("MIGRATION.md"),
        report::plan_markdown(report, plan, Some(&manifest)),
    )?;
    fs::write(
        out.join("README.md"),
        project_readme(report, &crate_name, web, &manifest, route_count),
    )?;

    let stubs: usize = stub_counts.values().sum();
    Ok(Outcome {
        crate_name,
        out_dir: out.clone(),
        models: struct_names.len(),
        traits: traits.len(),
        modules: manifest.modules.len(),
        stubs,
        routes: if web { route_count } else { 0 },
        manifest,
    })
}

fn project_readme(
    report: &Report,
    crate_name: &str,
    web: bool,
    manifest: &Manifest,
    routes: usize,
) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "# {crate_name}\n\nRust rebuild of **{}** ({}), scaffolded by [convert-to-rust]({}) `c2r` v{}.\n", report.project_name, report.primary_language.map(|l| l.name()).unwrap_or("unknown language"), crate::REPO_URL, env!("CARGO_PKG_VERSION"));
    s.push_str("## Status\n\nThis crate compiles and runs, but every ported function is still a typed `todo!()` stub with the original code embedded as comments. Work through `MIGRATION.md` phase by phase.\n\n");
    let _ = writeln!(s, "| | |\n|---|---|\n| Ported modules | {} |\n| Stubs to implement | {} |\n| HTTP routes | {} |\n", manifest.modules.len(), manifest.modules.iter().map(|m| m.stubs).sum::<usize>(), routes);
    s.push_str("## Run\n\n```bash\ncargo build\n");
    if web {
        s.push_str("cargo run              # serves on http://localhost:3000 (PORT to change)\ncurl -i localhost:3000/<any-original-route>   # 501 until ported\n");
    } else {
        s.push_str("cargo run\n");
    }
    s.push_str("```\n\n## Finish the port\n\n");
    s.push_str("- **By hand:** open a module in `src/ported/`, read the embedded original, replace `todo!()`.\n");
    s.push_str("- **With any LLM:** each file in `prompts/` is a self-contained translation prompt (context, target skeleton, original source). Paste it, then paste the answer over the module file.\n");
    s.push_str("- **Automatically:** `c2r translate .` (needs `ANTHROPIC_API_KEY`) translates every module in plan order and runs `cargo check`.\n\n");
    s.push_str("## Files\n\n- `ANALYSIS.md` - what c2r found in the original project\n- `MIGRATION.md` - phased plan, crate mapping, concept guide, risks\n- `c2r-report.json` / `c2r-manifest.json` - machine-readable analysis and module map\n- `prompts/` - per-module translation prompts\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn naming() {
        assert_eq!(snake("getUserByID"), "get_user_by_id");
        assert_eq!(snake("HTTPServer"), "http_server");
        assert_eq!(snake("type"), "type_");
        assert_eq!(snake("user-service.v2"), "user_service_v2");
        assert_eq!(snake("2fa"), "n_2fa");
        assert_eq!(pascal("loan_request"), "LoanRequest");
        assert_eq!(pascal("UserDTO"), "UserDTO");
        assert_eq!(module_name("user.service"), "user_service");
        assert_eq!(module_name("std"), "std_mod");
    }

    #[test]
    fn paths() {
        assert_eq!(axum_path("/users/:id"), "/users/{id}");
        assert_eq!(axum_path("/loans/<int:loan_id>"), "/loans/{loan_id}");
        assert_eq!(axum_path("/files/{file_path:path}"), "/files/{*file_path}");
        assert_eq!(
            axum_path("/api/{userId}/orders/{orderId}"),
            "/api/{user_id}/orders/{order_id}"
        );
        assert_eq!(axum_path("users/<pk>/"), "/users/{pk}/");
        assert_eq!(axum_path("*"), "/{*rest}");
        assert_eq!(axum_path("/"), "/");
    }

    #[test]
    fn segments() {
        assert_eq!(
            module_segments("src/services/__init__.py", 1),
            vec!["services"]
        );
        assert_eq!(
            module_segments("src/routes/user.routes.ts", 1),
            vec!["routes", "user_routes"]
        );
    }
}
