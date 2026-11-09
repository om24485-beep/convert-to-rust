//! End-to-end tests: run the real `c2r` binary on the bundled examples.

use std::path::{Path, PathBuf};
use std::process::Command;

fn c2r() -> Command {
    Command::new(env!("CARGO_BIN_EXE_c2r"))
}

fn example(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join(name)
}

fn scaffold(name: &str) -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join(format!("{name}-rs"));
    let status = c2r()
        .arg("scaffold")
        .arg(example(name))
        .arg("-o")
        .arg(&out)
        .status()
        .unwrap();
    assert!(status.success(), "scaffold failed for {name}");
    (tmp, out)
}

fn read(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

#[test]
fn analyze_reports_framework_and_routes() {
    let out = c2r()
        .arg("analyze")
        .arg(example("python-flask-loans"))
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("Flask"), "{text}");
    assert!(text.contains("HTTP routes      : 5"), "{text}");
}

#[test]
fn python_flask_scaffold() {
    let (_tmp, out) = scaffold("python-flask-loans");
    for f in [
        "Cargo.toml",
        "src/main.rs",
        "src/models.rs",
        "src/routes.rs",
        "src/ported/loans/emi.rs",
        "MIGRATION.md",
        "ANALYSIS.md",
        "c2r-manifest.json",
        "prompts/README.md",
    ] {
        assert!(out.join(f).exists(), "missing {f}");
    }
    let models = read(&out.join("src/models.rs"));
    assert!(models.contains("pub monthly_income: rust_decimal::Decimal"));
    assert!(models.contains("#[serde(rename = \"approved\")]"));
    assert!(
        models.contains("pub customer: Customer,"),
        "no needless Box:\n{models}"
    );
    let routes = read(&out.join("src/routes.rs"));
    assert!(routes.contains(".route(\"/loans/{loan_id}\", get(get_loan))"));
    let emi = read(&out.join("src/ported/loans/emi.rs"));
    assert!(emi.contains("pub fn emi(principal: rust_decimal::Decimal, annual_rate: rust_decimal::Decimal, tenure_months: i32) -> rust_decimal::Decimal"));
    assert!(emi.contains("// ---- original (Python) ----"));
}

#[test]
fn typescript_express_scaffold() {
    let (_tmp, out) = scaffold("node-express-users");
    let models = read(&out.join("src/models.rs"));
    assert!(models.contains("#[serde(rename = \"displayName\")]"));
    assert!(models.contains("pub last_login_at: Option<chrono::DateTime<chrono::Utc>>"));
    let routes = read(&out.join("src/routes.rs"));
    // Router mounted with app.use('/users', usersRouter)
    assert!(
        routes.contains(".route(\"/users/{id}\", get(get_user))"),
        "{routes}"
    );
    let svc = read(&out.join("src/ported/services/user_service.rs"));
    assert!(
        svc.contains("pub async fn find_by_id(&self, id: String) -> Option<User>"),
        "{svc}"
    );
}

#[test]
fn go_gin_scaffold() {
    let (_tmp, out) = scaffold("go-ledger");
    let models = read(&out.join("src/models.rs"));
    assert!(
        models.contains("pub balance_paise: i64"),
        "json tag used as field name:\n{models}"
    );
    let ledger = read(&out.join("src/ported/internal/ledger/ledger.rs"));
    assert!(
        ledger.contains("pub fn get(&self, id: String) -> anyhow::Result<Account>"),
        "{ledger}"
    );
    assert!(
        ledger.contains("pub fn submit(&self, t: Transfer) -> anyhow::Result<()>"),
        "{ledger}"
    );
    let routes = read(&out.join("src/routes.rs"));
    assert!(routes.contains("\"/accounts/{id}\""));
}

#[test]
fn mappings_command() {
    let out = c2r().args(["mappings", "pandas"]).output().unwrap();
    assert!(String::from_utf8_lossy(&out.stdout).contains("polars"));
}

/// Compiles every generated example. Slow and needs crates.io access:
/// `cargo test -- --ignored`
#[test]
#[ignore]
fn generated_projects_compile() {
    for name in ["python-flask-loans", "node-express-users", "go-ledger"] {
        let (_tmp, out) = scaffold(name);
        let status = Command::new("cargo")
            .args(["check", "--quiet"])
            .current_dir(&out)
            .status()
            .unwrap();
        assert!(status.success(), "{name} scaffold does not compile");
    }
}
