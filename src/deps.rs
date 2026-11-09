//! Dependency manifest parsers for the common package ecosystems.

use crate::model::Dependency;
use regex::Regex;
use std::path::Path;
use std::sync::LazyLock;

macro_rules! re {
    ($name:ident, $pat:expr) => {
        static $name: LazyLock<Regex> =
            LazyLock::new(|| Regex::new($pat).expect(stringify!($name)));
    };
}

re!(
    REQ_LINE,
    r"^\s*([A-Za-z0-9][A-Za-z0-9._\-]*)(?:\[[^\]]*\])?\s*(?:([=<>!~]=?|===)\s*([^\s;#,]+))?"
);
re!(SETUP_REQ, r"(?s)install_requires\s*=\s*\[(.*?)\]");
re!(QUOTED, r#"['"]([^'"]+)['"]"#);
re!(GO_REQ_BLOCK, r"(?s)require\s*\((.*?)\)");
re!(
    GO_REQ_LINE,
    r"(?m)^\s*([\w\.\-/]+)\s+(v[\w\.\-+]+)(\s*//\s*indirect)?"
);
re!(
    GO_REQ_SINGLE,
    r"(?m)^require\s+([\w\.\-/]+)\s+(v[\w\.\-+]+)"
);
re!(POM_DEP, r"(?s)<dependency>(.*?)</dependency>");
re!(POM_GROUP, r"<groupId>\s*([^<]+?)\s*</groupId>");
re!(POM_ARTIFACT, r"<artifactId>\s*([^<]+?)\s*</artifactId>");
re!(POM_VERSION, r"<version>\s*([^<]+?)\s*</version>");
re!(POM_SCOPE, r"<scope>\s*([^<]+?)\s*</scope>");
re!(
    GRADLE_DEP,
    r#"(?m)^\s*(implementation|api|compileOnly|runtimeOnly|testImplementation|testRuntimeOnly|kapt|ksp|annotationProcessor)\s*\(?\s*['"]([^:'"]+):([^:'"]+)(?::([^'"]+))?['"]"#
);
re!(
    GEMFILE,
    r#"(?m)^\s*gem\s+['"]([^'"]+)['"](?:\s*,\s*['"]([^'"]+)['"])?"#
);
re!(
    CSPROJ,
    r#"<PackageReference\s+Include="([^"]+)"(?:\s+Version="([^"]+)")?"#
);
re!(GO_MODULE, r"(?m)^module\s+(\S+)");

/// Returns true if the file name is a manifest this module understands.
pub fn is_manifest(name: &str) -> bool {
    matches!(
        name,
        "package.json"
            | "requirements.txt"
            | "requirements-dev.txt"
            | "dev-requirements.txt"
            | "pyproject.toml"
            | "setup.py"
            | "Pipfile"
            | "go.mod"
            | "pom.xml"
            | "build.gradle"
            | "build.gradle.kts"
            | "Gemfile"
            | "composer.json"
            | "Cargo.toml"
    ) || name.ends_with(".csproj")
        || (name.starts_with("requirements") && name.ends_with(".txt"))
}

pub struct ManifestInfo {
    pub deps: Vec<Dependency>,
    /// Entry points declared by the manifest (package.json main/bin, scripts).
    pub entries: Vec<String>,
    /// Go module path (used to resolve intra-project imports).
    pub go_module: Option<String>,
    pub project_name: Option<String>,
}

pub fn parse(path: &Path, rel: &str, text: &str) -> ManifestInfo {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let mut info = ManifestInfo {
        deps: vec![],
        entries: vec![],
        go_module: None,
        project_name: None,
    };
    let dep =
        |name: &str, group: Option<&str>, version: Option<&str>, eco: &str, dev: bool| Dependency {
            name: name.trim().to_string(),
            group: group.map(|g| g.trim().to_string()),
            version: version
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty()),
            ecosystem: eco.to_string(),
            dev,
            manifest: rel.to_string(),
        };

    match name {
        "package.json" | "composer.json" => {
            let eco = if name == "package.json" {
                "npm"
            } else {
                "composer"
            };
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(text) {
                info.project_name = v.get("name").and_then(|n| n.as_str()).map(str::to_string);
                let sections: &[(&str, bool)] = if eco == "npm" {
                    &[
                        ("dependencies", false),
                        ("devDependencies", true),
                        ("peerDependencies", false),
                        ("optionalDependencies", false),
                    ]
                } else {
                    &[("require", false), ("require-dev", true)]
                };
                for (section, dev) in sections {
                    if let Some(obj) = v.get(*section).and_then(|d| d.as_object()) {
                        for (k, ver) in obj {
                            if k == "php" || k.starts_with("ext-") {
                                continue;
                            }
                            info.deps.push(dep(k, None, ver.as_str(), eco, *dev));
                        }
                    }
                }
                if let Some(m) = v.get("main").and_then(|m| m.as_str()) {
                    info.entries.push(m.to_string());
                }
                match v.get("bin") {
                    Some(serde_json::Value::String(b)) => info.entries.push(b.clone()),
                    Some(serde_json::Value::Object(o)) => info
                        .entries
                        .extend(o.values().filter_map(|b| b.as_str().map(str::to_string))),
                    _ => {}
                }
            }
        }
        "pyproject.toml" | "Pipfile" | "Cargo.toml" => {
            let Ok(v) = text.parse::<toml::Table>() else {
                return info;
            };
            if name == "Cargo.toml" {
                info.project_name = v
                    .get("package")
                    .and_then(|p| p.get("name"))
                    .and_then(|n| n.as_str())
                    .map(str::to_string);
                for (section, dev) in [("dependencies", false), ("dev-dependencies", true)] {
                    if let Some(t) = v.get(section).and_then(|d| d.as_table()) {
                        for (k, val) in t {
                            let ver = val
                                .as_str()
                                .or_else(|| val.get("version").and_then(|x| x.as_str()));
                            info.deps.push(dep(k, None, ver, "cargo", dev));
                        }
                    }
                }
                return info;
            }
            if name == "Pipfile" {
                for (section, dev) in [("packages", false), ("dev-packages", true)] {
                    if let Some(t) = v.get(section).and_then(|d| d.as_table()) {
                        for (k, val) in t {
                            info.deps.push(dep(k, None, val.as_str(), "pypi", dev));
                        }
                    }
                }
                return info;
            }
            if let Some(project) = v.get("project") {
                info.project_name = project
                    .get("name")
                    .and_then(|n| n.as_str())
                    .map(str::to_string);
                if let Some(arr) = project.get("dependencies").and_then(|d| d.as_array()) {
                    for s in arr.iter().filter_map(|x| x.as_str()) {
                        if let Some(d) = parse_requirement(s, rel, false) {
                            info.deps.push(d);
                        }
                    }
                }
                if let Some(opt) = project
                    .get("optional-dependencies")
                    .and_then(|d| d.as_table())
                {
                    for (group, arr) in opt {
                        let dev =
                            matches!(group.as_str(), "dev" | "test" | "tests" | "lint" | "docs");
                        for s in arr
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|x| x.as_str())
                        {
                            if let Some(d) = parse_requirement(s, rel, dev) {
                                info.deps.push(d);
                            }
                        }
                    }
                }
                if let Some(scripts) = project.get("scripts").and_then(|s| s.as_table()) {
                    info.entries.extend(
                        scripts
                            .values()
                            .filter_map(|s| s.as_str())
                            .map(|s| s.split(':').next().unwrap_or(s).replace('.', "/") + ".py"),
                    );
                }
            }
            if let Some(poetry) = v.get("tool").and_then(|t| t.get("poetry")) {
                if info.project_name.is_none() {
                    info.project_name = poetry
                        .get("name")
                        .and_then(|n| n.as_str())
                        .map(str::to_string);
                }
                for (section, dev) in [("dependencies", false), ("dev-dependencies", true)] {
                    if let Some(t) = poetry.get(section).and_then(|d| d.as_table()) {
                        for (k, val) in t {
                            if k == "python" {
                                continue;
                            }
                            let ver = val
                                .as_str()
                                .or_else(|| val.get("version").and_then(|x| x.as_str()));
                            info.deps.push(dep(k, None, ver, "pypi", dev));
                        }
                    }
                }
                if let Some(groups) = poetry.get("group").and_then(|g| g.as_table()) {
                    for (_, g) in groups {
                        if let Some(t) = g.get("dependencies").and_then(|d| d.as_table()) {
                            for (k, val) in t {
                                info.deps.push(dep(k, None, val.as_str(), "pypi", true));
                            }
                        }
                    }
                }
            }
        }
        "setup.py" => {
            if let Some(c) = SETUP_REQ.captures(text) {
                for q in QUOTED.captures_iter(&c[1]) {
                    if let Some(d) = parse_requirement(&q[1], rel, false) {
                        info.deps.push(d);
                    }
                }
            }
        }
        "go.mod" => {
            info.go_module = GO_MODULE.captures(text).map(|c| c[1].to_string());
            info.project_name = info
                .go_module
                .as_ref()
                .and_then(|m| m.rsplit('/').next())
                .map(str::to_string);
            for block in GO_REQ_BLOCK.captures_iter(text) {
                for c in GO_REQ_LINE.captures_iter(&block[1]) {
                    info.deps
                        .push(dep(&c[1], None, Some(&c[2]), "go", c.get(3).is_some()));
                }
            }
            for c in GO_REQ_SINGLE.captures_iter(text) {
                info.deps.push(dep(&c[1], None, Some(&c[2]), "go", false));
            }
        }
        "pom.xml" => {
            for d in POM_DEP.captures_iter(text) {
                let body = &d[1];
                let (Some(g), Some(a)) = (POM_GROUP.captures(body), POM_ARTIFACT.captures(body))
                else {
                    continue;
                };
                let v = POM_VERSION.captures(body).map(|c| c[1].to_string());
                let dev = POM_SCOPE.captures(body).is_some_and(|c| &c[1] == "test");
                info.deps
                    .push(dep(&a[1], Some(&g[1]), v.as_deref(), "maven", dev));
            }
            info.project_name = POM_ARTIFACT.captures(text).map(|c| c[1].to_string());
        }
        "build.gradle" | "build.gradle.kts" => {
            for c in GRADLE_DEP.captures_iter(text) {
                let dev = c[1].starts_with("test");
                info.deps.push(dep(
                    &c[3],
                    Some(&c[2]),
                    c.get(4).map(|m| m.as_str()),
                    "maven",
                    dev,
                ));
            }
        }
        "Gemfile" => {
            for c in GEMFILE.captures_iter(text) {
                info.deps.push(dep(
                    &c[1],
                    None,
                    c.get(2).map(|m| m.as_str()),
                    "rubygems",
                    false,
                ));
            }
        }
        n if n.ends_with(".csproj") => {
            for c in CSPROJ.captures_iter(text) {
                info.deps.push(dep(
                    &c[1],
                    None,
                    c.get(2).map(|m| m.as_str()),
                    "nuget",
                    false,
                ));
            }
        }
        n if n.starts_with("requirements") || n.ends_with("requirements.txt") => {
            let dev = n.contains("dev") || n.contains("test");
            for line in text.lines() {
                let l = line.trim();
                if l.is_empty() || l.starts_with('#') || l.starts_with('-') {
                    continue;
                }
                if let Some(d) = parse_requirement(l, rel, dev) {
                    info.deps.push(d);
                }
            }
        }
        _ => {}
    }
    info
}

fn parse_requirement(s: &str, rel: &str, dev: bool) -> Option<Dependency> {
    let c = REQ_LINE.captures(s)?;
    Some(Dependency {
        name: c[1].to_string(),
        group: None,
        version: c.get(3).map(|v| {
            format!(
                "{}{}",
                c.get(2).map(|o| o.as_str()).unwrap_or(""),
                v.as_str()
            )
        }),
        ecosystem: "pypi".into(),
        dev,
        manifest: rel.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requirements() {
        let i = parse(
            Path::new("requirements.txt"),
            "requirements.txt",
            "flask==3.0.0\n# c\nrequests[socks]>=2.31\n-r base.txt\n",
        );
        assert_eq!(i.deps.len(), 2);
        assert_eq!(i.deps[1].name, "requests");
        assert_eq!(i.deps[1].version.as_deref(), Some(">=2.31"));
    }

    #[test]
    fn package_json() {
        let i = parse(
            Path::new("package.json"),
            "package.json",
            r#"{"name":"api","main":"src/index.js","dependencies":{"express":"^4"},"devDependencies":{"jest":"^29"}}"#,
        );
        assert_eq!(i.deps.len(), 2);
        assert!(i.deps[1].dev);
        assert_eq!(i.entries, vec!["src/index.js"]);
    }

    #[test]
    fn go_mod() {
        let i = parse(
            Path::new("go.mod"),
            "go.mod",
            "module github.com/acme/svc\n\ngo 1.22\n\nrequire (\n\tgithub.com/gin-gonic/gin v1.9.1\n\tgolang.org/x/sys v0.1.0 // indirect\n)\n",
        );
        assert_eq!(i.go_module.as_deref(), Some("github.com/acme/svc"));
        assert_eq!(i.deps.len(), 2);
        assert!(i.deps[1].dev);
    }

    #[test]
    fn pom() {
        let i = parse(
            Path::new("pom.xml"),
            "pom.xml",
            "<project><artifactId>loans</artifactId><dependencies><dependency><groupId>org.springframework.boot</groupId><artifactId>spring-boot-starter-web</artifactId></dependency></dependencies></project>",
        );
        assert_eq!(i.deps[0].name, "spring-boot-starter-web");
        assert_eq!(i.deps[0].group.as_deref(), Some("org.springframework.boot"));
    }
}
