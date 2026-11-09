//! Optional AI-assisted translation (feature `ai`, on by default).
//!
//! Sends each module's prompt to the Anthropic Messages API in plan order,
//! writes the returned Rust file over the stub (keeping a backup), then runs
//! `cargo check` and optionally feeds compiler errors back for repair rounds.

use crate::prompts::SYSTEM_PROMPT;
use crate::scaffold::{Manifest, ModuleEntry};
use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

pub struct TranslateOptions {
    pub model: String,
    pub only: Option<String>,
    pub max_modules: Option<usize>,
    pub fix_rounds: usize,
    pub check: bool,
    pub dry_run: bool,
}

struct Client {
    agent: ureq::Agent,
    key: String,
    base: String,
    model: String,
}

impl Client {
    fn from_env(model: &str) -> Result<Client> {
        let key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| anyhow!("ANTHROPIC_API_KEY is not set. Export your key, or translate by hand using the files in prompts/."))?;
        let base = std::env::var("ANTHROPIC_BASE_URL")
            .unwrap_or_else(|_| "https://api.anthropic.com".into());
        let base = base.trim_end_matches('/').to_string();
        let mut builder = ureq::AgentBuilder::new().timeout(Duration::from_secs(900));
        if let Some(proxy) = proxy_for(&base) {
            builder = builder.proxy(proxy);
        }
        Ok(Client {
            agent: builder.build(),
            key,
            base,
            model: model.to_string(),
        })
    }

    fn complete(&self, prompt: &str) -> Result<String> {
        let body = json!({
            "model": self.model,
            "max_tokens": 32000,
            "system": SYSTEM_PROMPT,
            "messages": [{ "role": "user", "content": prompt }],
        });
        let url = format!("{}/v1/messages", self.base);
        let mut delay = 5u64;
        for attempt in 1..=4 {
            let res = self
                .agent
                .post(&url)
                .set("x-api-key", &self.key)
                .set("anthropic-version", "2023-06-01")
                .set("content-type", "application/json")
                .send_json(body.clone());
            match res {
                Ok(r) => {
                    let v: Value = r.into_json().context("invalid JSON from API")?;
                    let text: String = v["content"]
                        .as_array()
                        .map(|blocks| {
                            blocks
                                .iter()
                                .filter(|b| b["type"] == "text")
                                .filter_map(|b| b["text"].as_str())
                                .collect::<Vec<_>>()
                                .join("")
                        })
                        .unwrap_or_default();
                    if text.is_empty() {
                        bail!("API returned no text (stop_reason: {})", v["stop_reason"]);
                    }
                    return Ok(text);
                }
                Err(ureq::Error::Status(code, r))
                    if matches!(code, 429 | 500 | 502 | 503 | 529) && attempt < 4 =>
                {
                    let _ = r.into_string();
                    eprintln!("  API busy ({code}), retrying in {delay}s ...");
                    std::thread::sleep(Duration::from_secs(delay));
                    delay *= 2;
                }
                Err(ureq::Error::Status(code, r)) => {
                    let msg = r.into_string().unwrap_or_default();
                    bail!("API error {code}: {msg}");
                }
                Err(e) => return Err(anyhow!("request failed: {e}")),
            }
        }
        bail!("API kept failing after retries")
    }
}

/// Proxy from the environment, ignoring empty values and honouring NO_PROXY.
fn proxy_for(url: &str) -> Option<ureq::Proxy> {
    let host = url
        .split("://")
        .nth(1)?
        .split(['/', ':'])
        .next()?
        .to_ascii_lowercase();
    if host == "localhost" || host == "127.0.0.1" || host == "::1" {
        return None;
    }
    let env = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
    let no_proxy = env("NO_PROXY")
        .or_else(|| env("no_proxy"))
        .unwrap_or_default();
    let bypass = no_proxy
        .split(',')
        .map(|p| {
            p.trim()
                .trim_start_matches('*')
                .trim_start_matches('.')
                .to_ascii_lowercase()
        })
        .any(|p| !p.is_empty() && (host == p || host.ends_with(&format!(".{p}"))))
        || no_proxy.trim() == "*";
    if bypass {
        return None;
    }
    let keys: &[&str] = if url.starts_with("https") {
        &["HTTPS_PROXY", "https_proxy", "ALL_PROXY", "all_proxy"]
    } else {
        &["HTTP_PROXY", "http_proxy", "ALL_PROXY", "all_proxy"]
    };
    keys.iter()
        .find_map(|k| env(k))
        .and_then(|p| ureq::Proxy::new(p).ok())
}

/// Extract the first ```rust (or plain ```) fenced block; fall back to the whole text.
pub fn extract_code(text: &str) -> Option<String> {
    for opener in ["```rust", "````rust", "```rs", "```"] {
        if let Some(start) = text.find(opener) {
            let after = &text[start + opener.len()..];
            let after = after
                .split_once('\n')
                .map(|(_, rest)| rest)
                .unwrap_or(after);
            let fence = if opener.starts_with("````") {
                "\n````"
            } else {
                "\n```"
            };
            let end = after.rfind(fence).unwrap_or(after.len());
            let code = after[..end].trim_end().to_string();
            if !code.trim().is_empty() {
                return Some(code + "\n");
            }
        }
    }
    let t = text.trim();
    (t.contains("fn ") || t.contains("use ")).then(|| t.to_string() + "\n")
}

pub fn translate(dir: &Path, opts: &TranslateOptions) -> Result<()> {
    let manifest_path = dir.join("c2r-manifest.json");
    let manifest: Manifest =
        serde_json::from_str(&fs::read_to_string(&manifest_path).with_context(|| {
            format!(
                "{} not found - run `c2r scaffold` first",
                manifest_path.display()
            )
        })?)?;
    let mut todo: Vec<&ModuleEntry> = manifest
        .modules
        .iter()
        .filter(|m| {
            opts.only
                .as_deref()
                .is_none_or(|pat| m.source.contains(pat) || m.rust_file.contains(pat))
        })
        .collect();
    if let Some(n) = opts.max_modules {
        todo.truncate(n);
    }
    if todo.is_empty() {
        bail!("no modules matched");
    }
    println!(
        "c2r translate: {} module(s) with {}",
        todo.len(),
        opts.model
    );
    if opts.dry_run {
        for m in &todo {
            println!(
                "  would translate {} -> {} (prompt {})",
                m.source, m.rust_file, m.prompt
            );
        }
        return Ok(());
    }
    let client = Client::from_env(&opts.model)?;
    let backup = dir.join(".c2r/backup");
    let mut needs: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut done = 0usize;
    for (i, m) in todo.iter().enumerate() {
        println!("[{}/{}] {} -> {}", i + 1, todo.len(), m.source, m.rust_file);
        let prompt = fs::read_to_string(dir.join(&m.prompt))
            .with_context(|| format!("missing prompt {}", m.prompt))?;
        let answer = match client.complete(&prompt) {
            Ok(a) => a,
            Err(e) => {
                eprintln!("  skipped: {e}");
                continue;
            }
        };
        let Some(code) = extract_code(&answer) else {
            eprintln!("  skipped: no Rust code in the response");
            continue;
        };
        let target = dir.join(&m.rust_file);
        let bk = backup.join(&m.rust_file);
        if let Some(p) = bk.parent() {
            fs::create_dir_all(p)?;
        }
        if target.exists() && !bk.exists() {
            fs::copy(&target, &bk)?;
        }
        for line in code
            .lines()
            .filter(|l| l.trim_start().starts_with("// c2r-needs:"))
        {
            for c in line.split(':').nth(1).unwrap_or("").split(',') {
                let c = c.trim().to_string();
                if !c.is_empty() {
                    needs.entry(c).or_default().push(m.rust_file.clone());
                }
            }
        }
        fs::write(&target, code)?;
        done += 1;
    }
    println!(
        "\nTranslated {done}/{} module(s); originals backed up in .c2r/backup/.",
        todo.len()
    );
    if done == 0 {
        bail!("no module was translated");
    }
    if !needs.is_empty() {
        println!(
            "\nModules asked for these crates (add them to Cargo.toml, e.g. with `cargo add`):"
        );
        for (c, files) in &needs {
            println!("  {c}  ({})", files.join(", "));
        }
    }
    if !opts.check {
        return Ok(());
    }
    for round in 0..=opts.fix_rounds {
        println!("\nRunning cargo check ...");
        let errors = cargo_errors(dir)?;
        if errors.is_empty() {
            println!("cargo check passed.");
            return Ok(());
        }
        let total: usize = errors.values().map(Vec::len).sum();
        println!("{total} error(s) in {} file(s).", errors.len());
        if round == opts.fix_rounds {
            for (f, errs) in &errors {
                println!("  {f}:");
                for e in errs.iter().take(5) {
                    println!("    {e}");
                }
            }
            println!("Fix the remaining errors by hand (backups of the stubs are in .c2r/backup).");
            return Ok(());
        }
        println!("Repair round {} of {} ...", round + 1, opts.fix_rounds);
        for (file, errs) in &errors {
            let path = dir.join(file);
            let Ok(current) = fs::read_to_string(&path) else {
                continue;
            };
            let prompt = format!(
                "The Rust file `{file}` below fails `cargo check` with these errors:\n\n```\n{}\n```\n\nReturn the complete corrected file in one ```rust code block. Keep all public item names; do not remove functionality.\n\n```rust\n{current}\n```",
                errs.join("\n")
            );
            match client.complete(&prompt).ok().and_then(|a| extract_code(&a)) {
                Some(code) => fs::write(&path, code)?,
                None => eprintln!("  could not repair {file}"),
            }
        }
    }
    Ok(())
}

/// Run `cargo check` and group short-format errors by file.
fn cargo_errors(dir: &Path) -> Result<BTreeMap<String, Vec<String>>> {
    let out = Command::new("cargo")
        .args(["check", "--quiet", "--message-format", "short"])
        .current_dir(dir)
        .output()
        .context("failed to run cargo")?;
    let text = String::from_utf8_lossy(&out.stderr);
    let mut map: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for line in text.lines() {
        if let Some((file, rest)) = line.split_once(':') {
            if file.ends_with(".rs") && rest.contains("error") {
                map.entry(file.to_string())
                    .or_default()
                    .push(line.to_string());
            }
        }
    }
    if map.is_empty() && !out.status.success() {
        map.insert(
            "Cargo.toml".into(),
            text.lines()
                .filter(|l| l.contains("error"))
                .map(str::to_string)
                .collect(),
        );
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_fenced_code() {
        let t = "Here you go:\n```rust\nfn main() {}\n```\nDone.";
        assert_eq!(extract_code(t).unwrap(), "fn main() {}\n");
        let t = "````rust\nuse x;\n```inner```\n````";
        assert!(extract_code(t).unwrap().contains("```inner```"));
    }
}
