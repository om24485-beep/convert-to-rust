//! Data model shared by every stage: analysis -> plan -> scaffold -> translate.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Language {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Go,
    Java,
    Kotlin,
    CSharp,
    C,
    Cpp,
    Ruby,
    Php,
    Swift,
    Scala,
    Shell,
    Sql,
    Html,
    Css,
    JavaBytecode,
}

impl Language {
    pub fn name(self) -> &'static str {
        match self {
            Language::Rust => "Rust",
            Language::Python => "Python",
            Language::JavaScript => "JavaScript",
            Language::TypeScript => "TypeScript",
            Language::Go => "Go",
            Language::Java => "Java",
            Language::Kotlin => "Kotlin",
            Language::CSharp => "C#",
            Language::C => "C",
            Language::Cpp => "C++",
            Language::Ruby => "Ruby",
            Language::Php => "PHP",
            Language::Swift => "Swift",
            Language::Scala => "Scala",
            Language::Shell => "Shell",
            Language::Sql => "SQL",
            Language::Html => "HTML",
            Language::Css => "CSS",
            Language::JavaBytecode => "Java bytecode",
        }
    }

    pub fn from_extension(ext: &str) -> Option<Language> {
        Some(match ext.to_ascii_lowercase().as_str() {
            "rs" => Language::Rust,
            "py" | "pyw" => Language::Python,
            "js" | "mjs" | "cjs" | "jsx" => Language::JavaScript,
            "ts" | "tsx" | "mts" | "cts" => Language::TypeScript,
            "go" => Language::Go,
            "java" => Language::Java,
            "kt" | "kts" => Language::Kotlin,
            "cs" => Language::CSharp,
            "c" | "h" => Language::C,
            "cpp" | "cc" | "cxx" | "hpp" | "hh" | "hxx" => Language::Cpp,
            "rb" => Language::Ruby,
            "php" => Language::Php,
            "swift" => Language::Swift,
            "scala" => Language::Scala,
            "sh" | "bash" | "zsh" => Language::Shell,
            "sql" => Language::Sql,
            "html" | "htm" => Language::Html,
            "css" | "scss" | "sass" | "less" => Language::Css,
            "class" => Language::JavaBytecode,
            _ => return None,
        })
    }

    /// Languages whose code c2r extracts symbols from and can port.
    pub fn is_portable(self) -> bool {
        matches!(
            self,
            Language::Python
                | Language::JavaScript
                | Language::TypeScript
                | Language::Go
                | Language::Java
                | Language::Kotlin
                | Language::CSharp
                | Language::C
                | Language::Cpp
                | Language::Ruby
                | Language::Php
        )
    }

    /// Block style used to find where a function body ends.
    pub fn uses_indentation_blocks(self) -> bool {
        matches!(self, Language::Python | Language::Ruby)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SymbolKind {
    Function,
    Method,
    Class,
    Struct,
    Interface,
    Enum,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Param {
    pub name: String,
    pub ty: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Field {
    pub name: String,
    pub ty: Option<String>,
    pub optional: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Symbol {
    pub kind: SymbolKind,
    pub name: String,
    pub parent: Option<String>,
    pub params: Vec<Param>,
    pub return_type: Option<String>,
    pub is_async: bool,
    pub line: usize,
    pub end_line: usize,
    /// Base classes / implemented interfaces (for types).
    pub bases: Vec<String>,
    /// Fields (for types) or enum variants (for enums, `ty` = None).
    pub fields: Vec<Field>,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Route {
    pub method: String,
    pub path: String,
    pub handler: Option<String>,
    pub framework: String,
    pub line: usize,
    /// Router variable the route was registered on (Express-style mounts).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receiver: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceFile {
    pub path: String,
    pub language: Language,
    pub lines: usize,
    pub code_lines: usize,
    pub is_entry: bool,
    pub is_test: bool,
    pub imports: Vec<String>,
    pub symbols: Vec<Symbol>,
    pub routes: Vec<Route>,
    /// Counts of language features that need special care in Rust.
    pub concepts: BTreeMap<String, usize>,
    /// Router mounts: (prefix, router variable), e.g. `app.use('/users', usersRouter)`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mounts: Vec<(String, String)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
    pub name: String,
    pub group: Option<String>,
    pub version: Option<String>,
    pub ecosystem: String,
    pub dev: bool,
    pub manifest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BinaryInfo {
    pub path: String,
    pub format: String,
    pub arch: String,
    pub bits: u8,
    pub size: u64,
    pub toolchain: Vec<String>,
    pub libraries: Vec<String>,
    pub imported_symbols: Vec<String>,
    pub exported_symbols: Vec<String>,
    pub interesting_strings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LangStats {
    pub files: usize,
    pub lines: usize,
    pub code_lines: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub tool: String,
    pub tool_version: String,
    pub project_name: String,
    pub target: String,
    pub input_kind: String,
    pub generated_unix: u64,
    pub primary_language: Option<Language>,
    pub languages: BTreeMap<String, LangStats>,
    pub frameworks: Vec<String>,
    pub entry_points: Vec<String>,
    pub files: Vec<SourceFile>,
    pub dependencies: Vec<Dependency>,
    pub binaries: Vec<BinaryInfo>,
    /// Java class names found in bytecode (JAR/WAR/APK inputs).
    pub bytecode_classes: Vec<String>,
    /// Module import graph: (from file, to file).
    pub module_graph: Vec<(String, String)>,
    pub other_files: usize,
    pub skipped_files: usize,
}

impl Report {
    pub fn total_code_lines(&self) -> usize {
        self.files.iter().map(|f| f.code_lines).sum()
    }

    pub fn total_symbols(&self) -> usize {
        self.files.iter().map(|f| f.symbols.len()).sum()
    }

    pub fn all_routes(&self) -> Vec<(&SourceFile, &Route)> {
        self.files
            .iter()
            .filter(|f| !f.is_test)
            .flat_map(|f| f.routes.iter().map(move |r| (f, r)))
            .collect()
    }

    pub fn concept_totals(&self) -> BTreeMap<String, usize> {
        let mut out = BTreeMap::new();
        for f in &self.files {
            for (k, v) in &f.concepts {
                *out.entry(k.clone()).or_insert(0) += v;
            }
        }
        out
    }
}
