//! Migration planning: dependency-ordered phases, effort estimates, risks and
//! the overall rebuild strategy.

use crate::mapping;
use crate::model::{Language, Report, SourceFile};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Serialize)]
pub struct PlannedModule {
    pub path: String,
    pub language: Language,
    pub code_lines: usize,
    pub symbols: usize,
    pub depends_on: Vec<String>,
    pub in_cycle: bool,
    pub effort_hours: f32,
    pub risks: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct Phase {
    pub number: usize,
    pub title: String,
    pub modules: Vec<PlannedModule>,
}

#[derive(Debug, Serialize)]
pub struct ConceptNote {
    pub key: String,
    pub title: String,
    pub occurrences: usize,
    pub guidance: String,
}

#[derive(Debug, Serialize)]
pub struct Plan {
    pub strategy: String,
    pub target_architecture: Vec<String>,
    pub phases: Vec<Phase>,
    pub total_hours: f32,
    pub unmapped_dependencies: Vec<String>,
    pub concepts: Vec<ConceptNote>,
    pub binary_notes: Vec<String>,
}

fn loc_per_hour(lang: Language) -> f32 {
    match lang {
        Language::Go => 60.0,
        Language::Java | Language::CSharp => 50.0,
        Language::JavaScript | Language::TypeScript | Language::Kotlin => 45.0,
        Language::Python | Language::Php => 40.0,
        Language::C | Language::Cpp | Language::Ruby => 35.0,
        _ => 40.0,
    }
}

fn module_risks(f: &SourceFile) -> Vec<String> {
    let mut r = vec![];
    let get = |k: &str| f.concepts.get(k).copied().unwrap_or(0);
    if get("dynamic_eval") > 0 {
        r.push(format!(
            "dynamic eval/metaprogramming x{}",
            get("dynamic_eval")
        ));
    }
    if get("reflection") > 0 {
        r.push(format!("reflection x{}", get("reflection")));
    }
    if get("manual_memory") > 0 {
        r.push(format!("manual memory x{}", get("manual_memory")));
    }
    if get("unsafe_pointers") > 5 {
        r.push("heavy pointer use".into());
    }
    if get("global_state") > 0 {
        r.push("global mutable state".into());
    }
    if get("threads") > 0 || get("goroutines") > 0 {
        r.push("concurrency".into());
    }
    if f.symbols.iter().any(|s| !s.bases.is_empty()) {
        r.push("inheritance -> traits".into());
    }
    r
}

pub fn build(report: &Report) -> Plan {
    let nodes: Vec<&SourceFile> = report
        .files
        .iter()
        .filter(|f| f.language.is_portable() && !f.is_test)
        .collect();
    let node_set: BTreeSet<&str> = nodes.iter().map(|f| f.path.as_str()).collect();
    let mut deps: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for (from, to) in &report.module_graph {
        if node_set.contains(from.as_str()) && node_set.contains(to.as_str()) && from != to {
            deps.entry(from.as_str()).or_default().insert(to.as_str());
        }
    }

    // Longest-path levelling with cycle breaking.
    let mut level: BTreeMap<&str, usize> = BTreeMap::new();
    let mut cyclic: BTreeSet<&str> = BTreeSet::new();
    while level.len() < nodes.len() {
        let mut progressed = false;
        for f in &nodes {
            let p = f.path.as_str();
            if level.contains_key(p) {
                continue;
            }
            let ds = deps.get(p).cloned().unwrap_or_default();
            if ds.iter().all(|d| level.contains_key(d)) {
                let l = ds.iter().map(|d| level[d] + 1).max().unwrap_or(0);
                level.insert(p, l);
                progressed = true;
            }
        }
        if !progressed {
            // Break a cycle at the node with the fewest unresolved deps.
            let pick = nodes
                .iter()
                .map(|f| f.path.as_str())
                .filter(|p| !level.contains_key(p))
                .min_by_key(|p| {
                    deps.get(p)
                        .map(|ds| ds.iter().filter(|d| !level.contains_key(*d)).count())
                        .unwrap_or(0)
                })
                .unwrap();
            let l = deps
                .get(pick)
                .map(|ds| {
                    ds.iter()
                        .filter_map(|d| level.get(d))
                        .map(|x| x + 1)
                        .max()
                        .unwrap_or(0)
                })
                .unwrap_or(0);
            level.insert(pick, l);
            cyclic.insert(pick);
        }
    }

    let entries: BTreeSet<&str> = report.entry_points.iter().map(String::as_str).collect();
    let max_level = level
        .iter()
        .filter(|(p, _)| !entries.contains(*p))
        .map(|(_, l)| *l)
        .max()
        .unwrap_or(0);
    let mut by_level: BTreeMap<usize, Vec<PlannedModule>> = BTreeMap::new();
    let mut total = 0.0f32;
    for f in &nodes {
        let p = f.path.as_str();
        let mut l = level.get(p).copied().unwrap_or(0);
        if entries.contains(p) {
            l = max_level + 1;
        }
        let risks = module_risks(f);
        let multiplier = 1.0 + 0.15 * risks.len() as f32;
        let hours = ((f.code_lines as f32 / loc_per_hour(f.language))
            + 0.25 * f.symbols.len() as f32)
            * multiplier;
        let hours = (hours * 4.0).ceil() / 4.0;
        total += hours;
        by_level.entry(l).or_default().push(PlannedModule {
            path: f.path.clone(),
            language: f.language,
            code_lines: f.code_lines,
            symbols: f.symbols.len(),
            depends_on: deps
                .get(p)
                .map(|d| d.iter().map(|s| s.to_string()).collect())
                .unwrap_or_default(),
            in_cycle: cyclic.contains(p),
            effort_hours: hours,
            risks,
        });
    }

    let n_levels = by_level.len();
    let mut phases: Vec<Phase> = by_level
        .into_iter()
        .enumerate()
        .map(|(i, (_, mut modules))| {
            modules.sort_by(|a, b| a.path.cmp(&b.path));
            let is_last = i + 1 == n_levels;
            let title = if i == 0 && n_levels > 1 {
                "Foundations - leaf modules, shared types and utilities".to_string()
            } else if is_last && modules.iter().any(|m| entries.contains(m.path.as_str())) {
                "Entry points and wiring (main, routers, startup)".to_string()
            } else if n_levels == 1 {
                "Port all modules".to_string()
            } else {
                format!("Layer {i} - modules that depend only on earlier phases")
            };
            Phase {
                number: i + 1,
                title,
                modules,
            }
        })
        .collect();

    let tests: Vec<PlannedModule> = report
        .files
        .iter()
        .filter(|f| f.is_test && f.language.is_portable())
        .map(|f| {
            let hours =
                ((f.code_lines as f32 / (loc_per_hour(f.language) * 1.5)) * 4.0).ceil() / 4.0;
            total += hours;
            PlannedModule {
                path: f.path.clone(),
                language: f.language,
                code_lines: f.code_lines,
                symbols: f.symbols.len(),
                depends_on: vec![],
                in_cycle: false,
                effort_hours: hours,
                risks: vec![],
            }
        })
        .collect();
    if !tests.is_empty() {
        let n = phases.len() + 1;
        phases.push(Phase {
            number: n,
            title: "Tests - port as #[cfg(test)] modules and tests/ integration tests".into(),
            modules: tests,
        });
    }

    let unmapped_dependencies: Vec<String> = mapping::map_dependencies(report)
        .into_iter()
        .filter(|m| m.crates.is_empty() && !m.dev && m.ecosystem != "import")
        .map(|m| format!("{} ({})", m.source, m.ecosystem))
        .collect();

    let mut concepts: Vec<ConceptNote> = report
        .concept_totals()
        .into_iter()
        .filter_map(|(k, n)| {
            mapping::concept_guidance(&k).map(|(title, g)| ConceptNote {
                key: k.clone(),
                title: title.into(),
                occurrences: n,
                guidance: g.into(),
            })
        })
        .collect();
    concepts.sort_by_key(|c| std::cmp::Reverse(c.occurrences));

    let web = mapping::is_web_project(report);
    let has_source = !nodes.is_empty();
    let strategy = if !has_source && !report.binaries.is_empty() {
        "Black-box re-implementation. No source code was found, so c2r cannot translate code directly. Use the recovered imports, exported symbols and strings below to specify behaviour, capture the binary's real inputs/outputs as golden test fixtures, then implement the Rust version until it reproduces them. For a full decompilation, run the binary through Ghidra/IDA (or REA) first and feed the result back into c2r.".to_string()
    } else if !has_source && !report.bytecode_classes.is_empty() {
        "Decompile first. This archive contains JVM bytecode only. Decompile it with CFR, Procyon, Vineflower or JADX into a folder of .java files, then run `c2r scaffold` on that folder for a full Rust skeleton.".to_string()
    } else if web {
        "Strangler-fig migration. Stand up the generated axum service beside the existing app, put both behind one reverse proxy, and move traffic route by route as each handler is ported. Keep the original as the reference: replay real requests against both and diff the responses until they match.".to_string()
    } else {
        "Bottom-up port. Translate leaf modules first (phase 1), keep each phase compiling with `cargo check`, and use the original test-suite (or recorded inputs/outputs) as the oracle for behaviour. Replace stubs one by one; `todo!()` panics show exactly what is still missing.".to_string()
    };

    let mut arch = vec![];
    if web {
        arch.push("HTTP: axum router + tokio runtime; middleware as tower layers; app state in `State<Arc<AppState>>`".to_string());
    }
    let crates = mapping::suggested_crates(report);
    let has = |n: &str| crates.contains_key(n);
    if has("sqlx") || has("sea-orm") {
        arch.push(
            "Persistence: sqlx connection pool (compile-time checked SQL) or SeaORM entities"
                .into(),
        );
    }
    if has("mongodb") {
        arch.push("Persistence: official mongodb driver with serde document structs".into());
    }
    if has("reqwest") {
        arch.push("Outbound HTTP: one shared reqwest::Client".into());
    }
    if has("clap") {
        arch.push("CLI: clap derive parser in main.rs".into());
    }
    if has("tracing") || web {
        arch.push("Observability: tracing + tracing-subscriber (structured logs, spans)".into());
    }
    arch.push(
        "Errors: thiserror enums per module, anyhow at the binary edge, `?` everywhere".into(),
    );
    arch.push(
        "Data: serde-derived structs (field renames preserve the original JSON contract)".into(),
    );
    if crates.contains_key("rust_decimal") {
        arch.push("Money: rust_decimal::Decimal (never f64)".into());
    }

    let mut binary_notes = vec![];
    for b in &report.binaries {
        let tc = if b.toolchain.is_empty() {
            "unknown toolchain".to_string()
        } else {
            b.toolchain.join(", ")
        };
        binary_notes.push(format!(
            "{} - {} {} {}-bit, built with {}; {} linked libraries, {} imports, {} exports",
            b.path,
            b.format,
            b.arch,
            b.bits,
            tc,
            b.libraries.len(),
            b.imported_symbols.len(),
            b.exported_symbols.len()
        ));
    }

    Plan {
        strategy,
        target_architecture: arch,
        phases,
        total_hours: total,
        unmapped_dependencies,
        concepts,
        binary_notes,
    }
}
