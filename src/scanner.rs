//! Input resolution (directory, single file, archive, binary or git URL) and
//! the project walk that produces a [`Report`].

use crate::model::{LangStats, Language, Report, SourceFile};
use crate::{binary, deps, extract, mapping, text};
use anyhow::{bail, Context, Result};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;
use walkdir::WalkDir;

const SKIP_DIRS: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    "node_modules",
    "target",
    "dist",
    "build",
    "out",
    "__pycache__",
    ".venv",
    "venv",
    "env",
    ".tox",
    ".mypy_cache",
    ".pytest_cache",
    "vendor",
    ".idea",
    ".vscode",
    ".next",
    ".nuxt",
    ".gradle",
    "bin",
    "obj",
    "coverage",
    ".cache",
    "bower_components",
    "Pods",
    "DerivedData",
    ".terraform",
    "site-packages",
];
const MAX_TEXT_BYTES: u64 = 2 * 1024 * 1024;
const MAX_BINARY_BYTES: u64 = 256 * 1024 * 1024;
const MAX_BINARIES: usize = 12;
const MAX_EXTRACT_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_EXTRACT_FILES: usize = 100_000;
const ZIP_EXTS: &[&str] = &[
    "zip", "jar", "war", "ear", "apk", "aar", "whl", "egg", "nupkg", "ipa", "xpi", "vsix", "crx",
];
const BINARY_EXTS: &[&str] = &["exe", "dll", "so", "dylib", "elf", "bin", "o", "node"];

/// Keeps temporary directories (clones, extracted archives) alive while the
/// report is in use.
pub struct Analysis {
    pub report: Report,
    pub root: PathBuf,
    _temp: Option<TempDir>,
}

impl Analysis {
    /// Read a project file by its report-relative path.
    pub fn read_source(&self, rel: &str) -> Option<String> {
        let p = self.root.join(rel);
        let p = if p.is_file() { p } else { self.root.clone() };
        fs::read(p)
            .ok()
            .map(|b| String::from_utf8_lossy(&b).to_string())
    }
}

pub fn analyze(target: &str) -> Result<Analysis> {
    let (root, single, kind, temp, display_name) = resolve(target)?;
    let mut report = Report {
        tool: "convert-to-rust (c2r)".into(),
        tool_version: env!("CARGO_PKG_VERSION").into(),
        project_name: display_name,
        target: target.to_string(),
        input_kind: kind.clone(),
        generated_unix: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        primary_language: None,
        languages: BTreeMap::new(),
        frameworks: vec![],
        entry_points: vec![],
        files: vec![],
        dependencies: vec![],
        binaries: vec![],
        bytecode_classes: vec![],
        module_graph: vec![],
        other_files: 0,
        skipped_files: 0,
    };

    let mut go_module: Option<String> = None;
    let mut manifest_entries: Vec<String> = vec![];

    let walker: Box<dyn Iterator<Item = PathBuf>> = match &single {
        Some(f) => Box::new(std::iter::once(f.clone())),
        None => Box::new(
            WalkDir::new(&root)
                .follow_links(false)
                .into_iter()
                .filter_entry(|e| {
                    e.depth() == 0
                        || !(e.file_type().is_dir()
                            && SKIP_DIRS.contains(&e.file_name().to_string_lossy().as_ref()))
                })
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file())
                .map(|e| e.into_path()),
        ),
    };

    for path in walker {
        let rel = rel_path(&root, &path);
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        let size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        if deps::is_manifest(&name) && size < MAX_TEXT_BYTES {
            if let Ok(txt) = fs::read_to_string(&path) {
                let info = deps::parse(&path, &rel, &txt);
                report.dependencies.extend(info.deps);
                manifest_entries.extend(info.entries);
                if go_module.is_none() {
                    go_module = info.go_module;
                }
            }
            if name != "setup.py" {
                continue;
            }
        }

        match Language::from_extension(&ext) {
            Some(Language::JavaBytecode) => {
                if !name.contains('$') {
                    report
                        .bytecode_classes
                        .push(rel.trim_end_matches(".class").replace('/', "."));
                }
                bump(&mut report.languages, Language::JavaBytecode, 0, 0);
            }
            Some(lang) => {
                if size > MAX_TEXT_BYTES {
                    report.skipped_files += 1;
                    continue;
                }
                let Ok(bytes) = fs::read(&path) else {
                    report.skipped_files += 1;
                    continue;
                };
                // Normalise CRLF so line-anchored patterns behave the same on every OS.
                let src = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
                if is_minified(&src) {
                    report.skipped_files += 1;
                    continue;
                }
                let lines = src.lines().count();
                let code = text::code_lines(&src);
                bump(&mut report.languages, lang, lines, code);
                if !lang.is_portable() {
                    if matches!(lang, Language::Rust | Language::Swift | Language::Scala) {
                        report.files.push(SourceFile {
                            path: rel.clone(),
                            language: lang,
                            lines,
                            code_lines: code,
                            is_entry: false,
                            is_test: is_test_path(&rel),
                            imports: vec![],
                            symbols: vec![],
                            routes: vec![],
                            concepts: BTreeMap::new(),
                            mounts: vec![],
                        });
                    }
                    continue;
                }
                let ex = extract::extract(lang, &src);
                let is_entry = detect_entry(lang, &name, &src);
                report.files.push(SourceFile {
                    path: rel.clone(),
                    language: lang,
                    lines,
                    code_lines: code,
                    is_entry,
                    is_test: is_test_path(&rel),
                    imports: ex.imports,
                    symbols: ex.symbols,
                    routes: ex.routes,
                    concepts: ex.concepts,
                    mounts: ex.mounts,
                });
            }
            None => {
                let candidate =
                    BINARY_EXTS.contains(&ext.as_str()) || ext.is_empty() || single.is_some();
                if candidate
                    && size > 64
                    && size <= MAX_BINARY_BYTES
                    && report.binaries.len() < MAX_BINARIES
                {
                    let mut head = [0u8; 8];
                    let ok = fs::File::open(&path)
                        .and_then(|mut f| f.read_exact(&mut head))
                        .is_ok();
                    if ok && binary::looks_like_binary(&head) {
                        if let Ok(bytes) = fs::read(&path) {
                            report.binaries.push(binary::analyze(&rel, &bytes));
                            continue;
                        }
                    }
                }
                report.other_files += 1;
            }
        }
    }

    // Entry points: detected in code + declared in manifests.
    for f in &report.files {
        if f.is_entry && !f.is_test {
            report.entry_points.push(f.path.clone());
        }
    }
    for e in manifest_entries {
        let e = e.trim_start_matches("./").to_string();
        if report.files.iter().any(|f| f.path == e) && !report.entry_points.contains(&e) {
            report.entry_points.push(e);
        }
    }
    for f in report.files.iter_mut() {
        if report.entry_points.contains(&f.path) {
            f.is_entry = true;
        }
    }

    report.primary_language = report
        .files
        .iter()
        .filter(|f| f.language.is_portable() || f.language == Language::Rust)
        .fold(BTreeMap::<Language, usize>::new(), |mut m, f| {
            *m.entry(f.language).or_default() += f.code_lines;
            m
        })
        .into_iter()
        .max_by_key(|(_, n)| *n)
        .map(|(l, _)| l)
        .or_else(|| (!report.bytecode_classes.is_empty()).then_some(Language::JavaBytecode));

    apply_mounts(&mut report);
    report.module_graph = module_graph(&report.files, go_module.as_deref());
    report.frameworks = mapping::detect_frameworks(&report);
    dedup_deps(&mut report);
    report.files.sort_by(|a, b| a.path.cmp(&b.path));

    if report.files.is_empty()
        && report.binaries.is_empty()
        && report.bytecode_classes.is_empty()
        && report.dependencies.is_empty()
    {
        bail!(
            "nothing to analyze in '{target}': no source files, manifests or binaries were found"
        );
    }
    Ok(Analysis {
        report,
        root,
        _temp: temp,
    })
}

type Resolved = (PathBuf, Option<PathBuf>, String, Option<TempDir>, String);

fn resolve(target: &str) -> Result<Resolved> {
    if is_git_url(target) {
        let dir = TempDir::new()?;
        eprintln!("c2r: cloning {target} ...");
        let status = Command::new("git")
            .args(["clone", "--depth", "1", "--quiet", target])
            .arg(dir.path().join("repo"))
            .status()
            .context("failed to run `git` (is it installed?)")?;
        if !status.success() {
            bail!("git clone failed for {target}");
        }
        let name = target
            .trim_end_matches('/')
            .trim_end_matches(".git")
            .rsplit('/')
            .next()
            .unwrap_or("project")
            .to_string();
        let root = dir.path().join("repo");
        return Ok((root, None, "git repository".into(), Some(dir), name));
    }

    let path = PathBuf::from(target);
    if !path.exists() {
        bail!("'{target}' does not exist (expected a directory, file, archive, binary or git URL)");
    }
    let stem = {
        let base = path
            .canonicalize()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_else(|| "project".into());
        if path.is_dir() {
            base
        } else {
            base.split('.').next().unwrap_or(&base).to_string()
        }
    };

    if path.is_dir() {
        return Ok((path, None, "source directory".into(), None, stem));
    }

    let lower = target.to_ascii_lowercase();
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ZIP_EXTS.contains(&ext.as_str()) {
        let dir = TempDir::new()?;
        extract_zip(&path, dir.path()).with_context(|| format!("failed to extract {target}"))?;
        let kind = match ext.as_str() {
            "jar" | "war" | "ear" => "Java archive",
            "apk" | "aar" => "Android package",
            "whl" | "egg" => "Python wheel",
            "nupkg" => "NuGet package",
            "ipa" => "iOS app archive",
            _ => "zip archive",
        };
        let root = dir.path().to_path_buf();
        return Ok((root, None, kind.into(), Some(dir), stem));
    }
    if lower.ends_with(".tar.gz")
        || lower.ends_with(".tgz")
        || lower.ends_with(".tar")
        || lower.ends_with(".crate")
    {
        let dir = TempDir::new()?;
        extract_tar(&path, dir.path(), !lower.ends_with(".tar"))
            .with_context(|| format!("failed to extract {target}"))?;
        let stem = stem.trim_end_matches(".tar").to_string();
        let root = dir.path().to_path_buf();
        return Ok((root, None, "tar archive".into(), Some(dir), stem));
    }

    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let mut head = [0u8; 8];
    let is_bin = fs::File::open(&path)
        .and_then(|mut f| f.read_exact(&mut head))
        .is_ok()
        && binary::looks_like_binary(&head);
    let kind = if is_bin {
        "compiled binary"
    } else {
        "single source file"
    };
    Ok((parent, Some(path), kind.into(), None, stem))
}

fn is_git_url(t: &str) -> bool {
    (t.starts_with("https://")
        || t.starts_with("http://")
        || t.starts_with("git@")
        || t.starts_with("ssh://"))
        && (t.ends_with(".git")
            || [
                "github.com",
                "gitlab.com",
                "bitbucket.org",
                "codeberg.org",
                "dev.azure.com",
            ]
            .iter()
            .any(|h| t.contains(h)))
}

fn extract_zip(archive: &Path, dest: &Path) -> Result<()> {
    let file = fs::File::open(archive)?;
    let mut zip = zip::ZipArchive::new(file)?;
    let mut total: u64 = 0;
    for i in 0..zip.len().min(MAX_EXTRACT_FILES) {
        let mut entry = zip.by_index(i)?;
        // enclosed_name() rejects absolute paths and `..` (zip-slip).
        let Some(rel) = entry.enclosed_name() else {
            continue;
        };
        let out = dest.join(rel);
        if entry.is_dir() {
            fs::create_dir_all(&out)?;
            continue;
        }
        total += entry.size();
        if total > MAX_EXTRACT_BYTES {
            bail!(
                "archive expands beyond {} MB; refusing (possible zip bomb)",
                MAX_EXTRACT_BYTES / 1024 / 1024
            );
        }
        if let Some(p) = out.parent() {
            fs::create_dir_all(p)?;
        }
        let mut f = fs::File::create(&out)?;
        std::io::copy(&mut (&mut entry).take(MAX_EXTRACT_BYTES), &mut f)?;
    }
    Ok(())
}

fn extract_tar(archive: &Path, dest: &Path, gz: bool) -> Result<()> {
    let file = fs::File::open(archive)?;
    let reader: Box<dyn Read> = if gz {
        Box::new(flate2::read::GzDecoder::new(file))
    } else {
        Box::new(file)
    };
    let mut ar = tar::Archive::new(reader);
    let mut total: u64 = 0;
    for (n, entry) in ar.entries()?.enumerate() {
        if n >= MAX_EXTRACT_FILES {
            break;
        }
        let mut entry = entry?;
        let path = entry.path()?.to_path_buf();
        if path.components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        }) {
            continue;
        }
        if !matches!(
            entry.header().entry_type(),
            tar::EntryType::Regular | tar::EntryType::Directory
        ) {
            continue; // skip symlinks, devices, etc.
        }
        total += entry.size();
        if total > MAX_EXTRACT_BYTES {
            bail!(
                "archive expands beyond {} MB; refusing",
                MAX_EXTRACT_BYTES / 1024 / 1024
            );
        }
        let out = dest.join(&path);
        if entry.header().entry_type() == tar::EntryType::Directory {
            fs::create_dir_all(&out)?;
        } else {
            if let Some(p) = out.parent() {
                fs::create_dir_all(p)?;
            }
            entry.unpack(&out)?;
        }
    }
    Ok(())
}

fn rel_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn bump(map: &mut BTreeMap<String, LangStats>, lang: Language, lines: usize, code: usize) {
    let s = map.entry(lang.name().to_string()).or_default();
    s.files += 1;
    s.lines += lines;
    s.code_lines += code;
}

fn is_minified(src: &str) -> bool {
    let lines = src.lines().count().max(1);
    src.len() > 20_000 && src.len() / lines > 500
}

pub fn is_test_path(rel: &str) -> bool {
    let l = rel.to_ascii_lowercase();
    let name = l.rsplit('/').next().unwrap_or(&l);
    l.split('/').any(|seg| {
        matches!(
            seg,
            "test" | "tests" | "__tests__" | "spec" | "specs" | "testdata"
        )
    }) || name.starts_with("test_")
        || name.contains("_test.")
        || name.contains(".test.")
        || name.contains(".spec.")
        || name.ends_with("test.java")
        || name.ends_with("tests.cs")
        || name.ends_with("test.kt")
        || name.ends_with("_spec.rb")
        || name == "conftest.py"
}

fn detect_entry(lang: Language, name: &str, src: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name);
    match lang {
        Language::Python => {
            src.contains("__name__ == \"__main__\"")
                || src.contains("__name__ == '__main__'")
                || matches!(name, "manage.py" | "__main__.py" | "wsgi.py" | "asgi.py")
                || (matches!(stem, "app" | "main")
                    && (src.contains("Flask(") || src.contains("FastAPI(")))
        }
        Language::JavaScript | Language::TypeScript => {
            matches!(stem, "server" | "main" | "app" | "index")
                && (src.contains(".listen(")
                    || src.contains("createServer")
                    || src.contains("NestFactory")
                    || src.contains("process.argv"))
        }
        Language::Go => src.contains("package main") && src.contains("func main()"),
        Language::Java | Language::Kotlin => {
            src.contains("static void main(")
                || src.contains("fun main(")
                || src.contains("@SpringBootApplication")
        }
        Language::CSharp => {
            src.contains("static void Main(")
                || src.contains("static async Task Main(")
                || (name == "Program.cs")
        }
        Language::C | Language::Cpp => {
            src.contains("int main(") || src.contains("int main (") || src.contains("void main(")
        }
        Language::Ruby => {
            matches!(name, "config.ru" | "app.rb") || src.contains("if __FILE__ == $0")
        }
        Language::Php => matches!(name, "index.php" | "artisan"),
        _ => false,
    }
}

/// Resolve imports between project files into (from, to) edges.
fn module_graph(files: &[SourceFile], go_module: Option<&str>) -> Vec<(String, String)> {
    // Lookup keys -> file indexes.
    let mut keys: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, f) in files.iter().enumerate() {
        let no_ext = f
            .path
            .rsplit_once('.')
            .map(|(a, _)| a)
            .unwrap_or(&f.path)
            .to_string();
        let mut push = |k: String| keys.entry(k).or_default().push(i);
        push(no_ext.clone());
        let dir = f
            .path
            .rsplit_once('/')
            .map(|(d, _)| d.to_string())
            .unwrap_or_default();
        let stem = no_ext.rsplit('/').next().unwrap_or("");
        if matches!(stem, "index" | "__init__" | "mod") || f.language == Language::Go {
            push(dir.clone());
        }
        // Java/Kotlin: strip leading src/main/java etc.
        for marker in ["src/main/java/", "src/main/kotlin/", "src/", "lib/", "app/"] {
            if let Some(rest) = no_ext.strip_prefix(marker) {
                push(rest.to_string());
            }
        }
    }

    let mut edges: BTreeSet<(String, String)> = BTreeSet::new();
    for (i, f) in files.iter().enumerate() {
        let dir = f.path.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
        for imp in &f.imports {
            let candidates: Vec<String> = match f.language {
                Language::JavaScript
                | Language::TypeScript
                | Language::Ruby
                | Language::Php
                | Language::C
                | Language::Cpp => {
                    if imp.starts_with('.') {
                        vec![normalize(&format!("{dir}/{imp}"))]
                    } else {
                        let base = imp
                            .trim_end_matches(".rb")
                            .trim_end_matches(".php")
                            .trim_end_matches(".h")
                            .trim_end_matches(".hpp");
                        vec![base.to_string(), normalize(&format!("{dir}/{base}"))]
                    }
                }
                Language::Python => {
                    let dots = imp.chars().take_while(|c| *c == '.').count();
                    let rest = imp[dots..].replace('.', "/");
                    if dots > 0 {
                        let mut base = dir.to_string();
                        for _ in 1..dots {
                            base = base
                                .rsplit_once('/')
                                .map(|(a, _)| a.to_string())
                                .unwrap_or_default();
                        }
                        vec![normalize(&format!("{base}/{rest}"))]
                    } else {
                        vec![rest]
                    }
                }
                Language::Go => {
                    let local = go_module
                        .and_then(|m| imp.strip_prefix(m))
                        .map(|r| r.trim_start_matches('/').to_string());
                    local.into_iter().collect()
                }
                Language::Java | Language::Kotlin | Language::CSharp => {
                    vec![imp.trim_end_matches(".*").replace('.', "/")]
                }
                _ => vec![],
            };
            for cand in candidates {
                let cand = cand
                    .trim_end_matches(".js")
                    .trim_end_matches(".ts")
                    .trim_start_matches('/')
                    .to_string();
                let hit = keys.get(&cand).cloned().or_else(|| {
                    // Suffix match for package-relative imports.
                    if cand.contains('/') {
                        keys.iter()
                            .find(|(k, _)| k.ends_with(&format!("/{cand}")))
                            .map(|(_, v)| v.clone())
                    } else {
                        None
                    }
                });
                if let Some(targets) = hit {
                    for t in targets {
                        if t != i {
                            edges.insert((f.path.clone(), files[t].path.clone()));
                        }
                    }
                    break;
                }
            }
        }
    }
    edges.into_iter().collect()
}

/// Prefix routes registered on a router variable that is mounted elsewhere
/// (`app.use('/users', usersRouter)`).
fn apply_mounts(report: &mut Report) {
    let mounts: Vec<(String, String)> =
        report.files.iter().flat_map(|f| f.mounts.clone()).collect();
    if mounts.is_empty() {
        return;
    }
    for f in report.files.iter_mut() {
        for r in f.routes.iter_mut() {
            let Some(recv) = &r.receiver else { continue };
            if let Some((prefix, _)) = mounts.iter().find(|(_, var)| var == recv) {
                let joined = format!(
                    "{}/{}",
                    prefix.trim_end_matches('/'),
                    r.path.trim_start_matches('/')
                );
                r.path = if joined.len() > 1 {
                    joined.trim_end_matches('/').to_string()
                } else {
                    joined
                };
            }
        }
    }
}

fn normalize(p: &str) -> String {
    let mut out: Vec<&str> = vec![];
    for seg in p.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            s => out.push(s),
        }
    }
    out.join("/")
}

fn dedup_deps(report: &mut Report) {
    let mut seen = BTreeSet::new();
    report
        .dependencies
        .retain(|d| seen.insert((d.ecosystem.clone(), d.name.to_ascii_lowercase(), d.dev)));
    report.dependencies.sort_by(|a, b| {
        (a.dev, &a.ecosystem, a.name.to_ascii_lowercase()).cmp(&(
            b.dev,
            &b.ecosystem,
            b.name.to_ascii_lowercase(),
        ))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_paths() {
        assert!(is_test_path("tests/test_api.py"));
        assert!(is_test_path("src/user.spec.ts"));
        assert!(is_test_path("pkg/store_test.go"));
        assert!(!is_test_path("src/latest.py"));
    }

    #[test]
    fn git_urls() {
        assert!(is_git_url("https://github.com/acme/app"));
        assert!(is_git_url("git@github.com:acme/app.git"));
        assert!(!is_git_url("./my-app"));
    }
}
