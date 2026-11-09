//! convert-to-rust (`c2r`): reverse engineer any project and rebuild it in Rust.

mod binary;
mod deps;
mod extract;
mod mapping;
mod model;
mod plan;
mod prompts;
mod report;
mod scaffold;
mod scanner;
mod text;

#[cfg(feature = "ai")]
mod ai;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use std::process::Command;

pub const REPO_URL: &str = "https://github.com/om24485-beep/convert-to-rust";
const DEFAULT_MODEL: &str = "claude-sonnet-5-5";

#[derive(Parser)]
#[command(
    name = "c2r",
    version,
    about = "Reverse engineer any project and rebuild it in Rust",
    long_about = "convert-to-rust (c2r) analyses a project - a source folder, a single file, a git URL, an archive (zip/jar/war/apk/whl/tar.gz) or a compiled binary - recovers its structure (modules, functions, types, HTTP routes, dependencies, risky language features) and generates a compiling Rust project with typed stubs, an axum router, a dependency-to-crate mapping, a phased migration plan and per-module translation prompts. Optionally it translates every module with an LLM and loops on `cargo check`.",
    after_help = "Examples:\n  c2r analyze ./my-flask-app\n  c2r convert https://github.com/user/repo -o repo-rs\n  c2r scaffold app.jar -o app-rs\n  c2r translate ./repo-rs            # needs ANTHROPIC_API_KEY\n  c2r mappings pandas"
)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Reverse engineer a project and print what was found
    Analyze {
        /// Directory, file, archive, binary or git URL
        target: String,
        /// Write the full analysis as JSON
        #[arg(long)]
        json: Option<PathBuf>,
        /// Write the analysis as Markdown
        #[arg(long)]
        markdown: Option<PathBuf>,
    },
    /// Produce a phased Rust migration plan (Markdown)
    Plan {
        target: String,
        /// Output file (prints to stdout when omitted)
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
    /// Generate a compiling Rust project from a project
    Scaffold {
        target: String,
        /// Output directory (default: ./<project>-rs)
        #[arg(short, long)]
        out: Option<PathBuf>,
        /// Crate name (default: <project>-rs)
        #[arg(long)]
        name: Option<String>,
        /// Activate all suggested crates in Cargo.toml instead of listing them as comments
        #[arg(long)]
        with_deps: bool,
        /// Do not embed the original code as comments inside each stub
        #[arg(long)]
        no_source: bool,
        /// Write into a non-empty output directory
        #[arg(long)]
        force: bool,
    },
    /// Analyze, then offer to rebuild in Rust (scaffold, optional AI translation, cargo check)
    Convert {
        target: String,
        #[arg(short, long)]
        out: Option<PathBuf>,
        #[arg(long)]
        name: Option<String>,
        /// Skip the confirmation prompt
        #[arg(short, long)]
        yes: bool,
        /// Translate every module with Claude after scaffolding (needs ANTHROPIC_API_KEY)
        #[arg(long)]
        ai: bool,
        /// Model for --ai
        #[arg(long, env = "C2R_MODEL", default_value = DEFAULT_MODEL)]
        model: String,
        /// Skip `cargo check` on the generated project
        #[arg(long)]
        no_check: bool,
        #[arg(long)]
        with_deps: bool,
        #[arg(long)]
        force: bool,
    },
    /// Translate the stubs of a scaffolded project with Claude, then cargo check
    Translate {
        /// Scaffolded project directory
        #[arg(default_value = ".")]
        dir: PathBuf,
        #[arg(long, env = "C2R_MODEL", default_value = DEFAULT_MODEL)]
        model: String,
        /// Only modules whose source or Rust path contains this text
        #[arg(long)]
        only: Option<String>,
        /// Translate at most N modules
        #[arg(long)]
        max: Option<usize>,
        /// Compiler-error repair rounds after translation
        #[arg(long, default_value_t = 2)]
        fix_rounds: usize,
        /// Skip cargo check
        #[arg(long)]
        no_check: bool,
        /// Show what would be translated
        #[arg(long)]
        dry_run: bool,
    },
    /// Show the package -> Rust crate knowledge base (optionally filtered)
    Mappings {
        /// Filter by package name, crate or category
        query: Option<String>,
    },
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

fn default_out(report: &model::Report) -> PathBuf {
    let base: String = report
        .project_name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    PathBuf::from(format!("{}-rs", base.trim_matches('-')))
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Cmd::Analyze {
            target,
            json,
            markdown,
        } => {
            let a = scanner::analyze(&target)?;
            let p = plan::build(&a.report);
            report::print_summary(&a.report, &p);
            if let Some(j) = json {
                std::fs::write(&j, serde_json::to_string_pretty(&a.report)?)?;
                println!("\n  JSON written to {}", j.display());
            }
            if let Some(m) = markdown {
                std::fs::write(&m, report::analysis_markdown(&a.report))?;
                println!("  Markdown written to {}", m.display());
            }
            println!("\nRebuild it in Rust:  c2r convert {target}");
        }
        Cmd::Plan { target, out } => {
            let a = scanner::analyze(&target)?;
            let p = plan::build(&a.report);
            let md = report::plan_markdown(&a.report, &p, None);
            match out {
                Some(o) => {
                    std::fs::write(&o, md)?;
                    println!("Plan written to {}", o.display());
                }
                None => print!("{md}"),
            }
        }
        Cmd::Scaffold {
            target,
            out,
            name,
            with_deps,
            no_source,
            force,
        } => {
            let a = scanner::analyze(&target)?;
            let p = plan::build(&a.report);
            report::print_summary(&a.report, &p);
            let out = out.unwrap_or_else(|| default_out(&a.report));
            let o = scaffold::generate(
                &a,
                &p,
                &scaffold::Options {
                    out_dir: out,
                    name,
                    with_deps,
                    embed_source: !no_source,
                    force,
                },
            )?;
            print_outcome(&o);
        }
        Cmd::Convert {
            target,
            out,
            name,
            yes,
            ai,
            model,
            no_check,
            with_deps,
            force,
        } => {
            let a = scanner::analyze(&target)?;
            let p = plan::build(&a.report);
            report::print_summary(&a.report, &p);
            let out = out.unwrap_or_else(|| default_out(&a.report));
            let mut use_ai = ai;
            if !yes && std::io::stdin().is_terminal() {
                println!();
                print!(
                    "Rebuild this project in Rust in '{}'?  [Y]es / [a]i-translate too / [n]o: ",
                    out.display()
                );
                std::io::stdout().flush()?;
                let mut answer = String::new();
                std::io::stdin().read_line(&mut answer)?;
                match answer.trim().to_ascii_lowercase().as_str() {
                    "" | "y" | "yes" => {}
                    "a" | "ai" => use_ai = true,
                    _ => {
                        println!("Nothing written. Run `c2r plan {target}` to read the migration plan first.");
                        return Ok(());
                    }
                }
            }
            let o = scaffold::generate(
                &a,
                &p,
                &scaffold::Options {
                    out_dir: out.clone(),
                    name,
                    with_deps,
                    embed_source: true,
                    force,
                },
            )?;
            print_outcome(&o);
            if use_ai {
                translate_cmd(&out, &model, None, None, 2, !no_check, false)?;
            } else if !no_check {
                cargo_check(&out);
            }
        }
        Cmd::Translate {
            dir,
            model,
            only,
            max,
            fix_rounds,
            no_check,
            dry_run,
        } => {
            translate_cmd(&dir, &model, only, max, fix_rounds, !no_check, dry_run)?;
        }
        Cmd::Mappings { query } => print_mappings(query.as_deref()),
    }
    Ok(())
}

#[cfg(feature = "ai")]
fn translate_cmd(
    dir: &std::path::Path,
    model: &str,
    only: Option<String>,
    max: Option<usize>,
    fix_rounds: usize,
    check: bool,
    dry_run: bool,
) -> Result<()> {
    ai::translate(
        dir,
        &ai::TranslateOptions {
            model: model.to_string(),
            only,
            max_modules: max,
            fix_rounds,
            check,
            dry_run,
        },
    )
}

#[cfg(not(feature = "ai"))]
fn translate_cmd(
    _: &std::path::Path,
    _: &str,
    _: Option<String>,
    _: Option<usize>,
    _: usize,
    _: bool,
    _: bool,
) -> Result<()> {
    anyhow::bail!("this build of c2r was compiled without the `ai` feature; use the prompts/ folder or rebuild with `--features ai`")
}

fn print_outcome(o: &scaffold::Outcome) {
    println!("\nRust project generated: {}", o.out_dir.display());
    println!("  crate      : {}", o.crate_name);
    println!(
        "  models     : {} structs/enums, {} traits",
        o.models, o.traits
    );
    println!("  modules    : {} ({} typed stubs)", o.modules, o.stubs);
    if o.routes > 0 {
        println!(
            "  routes     : {} axum routes (answer 501 until ported)",
            o.routes
        );
    }
    println!(
        "  docs       : ANALYSIS.md, MIGRATION.md, prompts/ ({} prompts)",
        o.manifest.modules.len()
    );
    println!("\nNext:");
    println!("  cd {} && cargo run", o.out_dir.display());
    println!(
        "  c2r translate {}     # AI-translate every stub (ANTHROPIC_API_KEY)",
        o.out_dir.display()
    );
}

fn cargo_check(dir: &std::path::Path) {
    print!("\nRunning cargo check on the generated project ... ");
    let _ = std::io::stdout().flush();
    match Command::new("cargo")
        .args(["check", "--quiet"])
        .current_dir(dir)
        .status()
    {
        Ok(s) if s.success() => println!("ok - it compiles."),
        Ok(_) => println!("failed (see errors above)."),
        Err(_) => println!("skipped (cargo not found)."),
    }
}

fn print_mappings(query: Option<&str>) {
    let q = query.map(|q| q.to_ascii_lowercase());
    let mut shown = 0;
    for r in mapping::RULES {
        let crates: Vec<String> = r
            .crates
            .iter()
            .map(|c| format!("{} {}", c.name, c.version))
            .collect();
        let hay = format!(
            "{} {} {}",
            r.names.join(" "),
            crates.join(" "),
            r.category.label()
        )
        .to_ascii_lowercase();
        if let Some(q) = &q {
            if !hay.contains(q.as_str()) {
                continue;
            }
        }
        println!("[{}] {}", r.category.label(), r.names.join(", "));
        println!("    -> {}", crates.join(", "));
        if !r.note.is_empty() {
            println!("       {}", r.note);
        }
        shown += 1;
    }
    if shown == 0 {
        println!(
            "No mapping matches. Check crates.io / lib.rs, or call the service's API with reqwest."
        );
    }
}
