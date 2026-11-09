//! Self-contained, per-module translation prompts usable with any LLM (or as a
//! checklist for a human porter). `c2r translate` sends exactly these.

use crate::mapping;
use crate::model::Language;
use crate::plan::Plan;
use crate::scaffold::Manifest;
use crate::scanner::Analysis;
use anyhow::Result;
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

pub const SYSTEM_PROMPT: &str = "You are an expert polyglot engineer porting software to idiomatic, safe, production-quality Rust (edition 2021). \
You translate behaviour faithfully: same inputs, same outputs, same side effects, same error cases. \
You never invent features, never leave todo!() where the original has logic, and you reply with exactly one complete Rust file in a single ```rust code block.";

fn fence(lang: Language) -> &'static str {
    match lang {
        Language::Python => "python",
        Language::JavaScript => "javascript",
        Language::TypeScript => "typescript",
        Language::Go => "go",
        Language::Java => "java",
        Language::Kotlin => "kotlin",
        Language::CSharp => "csharp",
        Language::C => "c",
        Language::Cpp => "cpp",
        Language::Ruby => "ruby",
        Language::Php => "php",
        _ => "text",
    }
}

pub fn write_all(analysis: &Analysis, plan: &Plan, manifest: &Manifest, out: &Path) -> Result<()> {
    let report = &analysis.report;
    let dir = out.join("prompts");
    fs::create_dir_all(&dir)?;
    let models = fs::read_to_string(out.join("src/models.rs")).unwrap_or_default();
    let models_excerpt: String = models.lines().take(400).collect::<Vec<_>>().join("\n");
    let mapped: Vec<String> = mapping::map_dependencies(report)
        .into_iter()
        .filter(|m| !m.dev && !m.crates.is_empty())
        .take(30)
        .map(|m| {
            format!(
                "| `{}` | {} | {} |",
                m.source,
                m.crates
                    .iter()
                    .map(|c| format!("`{}`", c.name))
                    .collect::<Vec<_>>()
                    .join(", "),
                m.note
            )
        })
        .collect();
    let std_hints: Vec<String> = mapping::std_hints_for(report)
        .into_iter()
        .map(|h| format!("- `{}` -> {}", h.module, h.rust))
        .collect();
    let concept_map: std::collections::BTreeMap<&str, &crate::plan::ConceptNote> =
        plan.concepts.iter().map(|c| (c.key.as_str(), c)).collect();

    let mut index = String::new();
    let _ = writeln!(index, "# Translation prompts for {}\n\nTranslate in this order (dependencies first). Each prompt is self-contained: paste it into any capable LLM and replace the target file with the answer, or run `c2r translate .` to do it automatically.\n", report.project_name);
    let _ = writeln!(
        index,
        "Use this as the system prompt (or paste it above each prompt):\n\n> {SYSTEM_PROMPT}\n"
    );
    let _ = writeln!(
        index,
        "| # | Phase | Original | Rust file | Stubs |\n|---|---|---|---|---|"
    );

    for (i, m) in manifest.modules.iter().enumerate() {
        let Some(file) = report.files.iter().find(|f| f.path == m.source) else {
            continue;
        };
        let source = analysis.read_source(&m.source).unwrap_or_default();
        let skeleton = fs::read_to_string(out.join(&m.rust_file)).unwrap_or_default();
        let mut p = String::new();
        let _ = writeln!(
            p,
            "# Translate `{}` ({}) to Rust\n",
            m.source,
            file.language.name()
        );
        let _ = writeln!(p, "You are porting **{}** to Rust, one module at a time. Translate the original module below into idiomatic Rust.\n", report.project_name);
        p.push_str("## Requirements\n\n");
        let _ = writeln!(p, "- Output the complete contents of `{}` (module `{}`) as a single ```rust code block, with no explanation.", m.rust_file, m.module_path);
        p.push_str("- Keep every public item and name from the skeleton so other modules still compile. You may tighten `serde_json::Value` placeholders into concrete types and add private helpers.\n");
        if manifest.models_file.is_some() {
            p.push_str("- Shared types live in `crate::models` (excerpt below). Do not redefine them; add inherent `impl` blocks here as the skeleton does.\n");
        }
        p.push_str("- Errors: return `anyhow::Result<T>` (or a `thiserror` enum) instead of throwing; use `?`. Use `Option<T>` for nullable values.\n");
        p.push_str("- Preserve behaviour exactly: validation rules, rounding, error messages, status codes, edge cases. Money/decimal values use `rust_decimal::Decimal`, never f64.\n");
        p.push_str("- Only use crates already in Cargo.toml or listed in the mapping below. Add `// c2r-needs: <crate>` comments at the top for any crate that must be added.\n");
        p.push_str(
            "- Remove the embedded `// ---- original ----` comment blocks once translated.\n\n",
        );
        if !mapped.is_empty() || !std_hints.is_empty() {
            p.push_str("## Library mapping for this project\n\n");
            if !mapped.is_empty() {
                p.push_str("| Original | Rust | Note |\n|---|---|---|\n");
                for row in &mapped {
                    let _ = writeln!(p, "{row}");
                }
                p.push('\n');
            }
            for h in &std_hints {
                let _ = writeln!(p, "{h}");
            }
            p.push('\n');
        }
        let concepts: BTreeSet<&str> = file.concepts.keys().map(String::as_str).collect();
        let notes: Vec<String> = concepts
            .iter()
            .filter_map(|k| concept_map.get(k))
            .map(|c| format!("- **{}**: {}", c.title, c.guidance))
            .collect();
        if !notes.is_empty() {
            p.push_str("## Idioms needed in this module\n\n");
            for n in notes {
                let _ = writeln!(p, "{n}");
            }
            p.push('\n');
        }
        if !models_excerpt.is_empty() {
            let _ = writeln!(
                p,
                "## Shared models (`src/models.rs`)\n\n````rust\n{}\n````\n",
                models_excerpt
            );
        }
        let deps: Vec<&String> = report
            .module_graph
            .iter()
            .filter(|(a, _)| a == &m.source)
            .map(|(_, b)| b)
            .collect();
        if !deps.is_empty() {
            p.push_str("## This module uses\n\n");
            for d in deps {
                if let Some(other) = manifest.modules.iter().find(|x| &x.source == d) {
                    let _ = writeln!(p, "- `{}` -> `{}`", d, other.module_path);
                } else {
                    let _ = writeln!(p, "- `{d}`");
                }
            }
            p.push('\n');
        }
        let _ = writeln!(
            p,
            "## Current skeleton (`{}`)\n\n````rust\n{}\n````\n",
            m.rust_file,
            skeleton.trim_end()
        );
        let _ = writeln!(
            p,
            "## Original source (`{}`)\n\n````{}\n{}\n````",
            m.source,
            fence(file.language),
            source.trim_end()
        );
        fs::write(out.join(&m.prompt), p)?;
        let _ = writeln!(
            index,
            "| {} | {} | `{}` | `{}` | {} |",
            i + 1,
            m.phase,
            m.source,
            m.rust_file,
            m.stubs
        );
    }
    fs::write(dir.join("README.md"), index)?;
    Ok(())
}
