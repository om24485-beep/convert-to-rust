//! Heuristic, dependency-free source extractors.
//!
//! Each language gets a handful of regexes that recover the *shape* of the
//! code: imports, functions, methods, types (with fields), HTTP routes and the
//! language features that need special treatment in Rust ("concepts").
//! They are deliberately tolerant: a missed symbol costs a stub, a crash costs
//! the whole run.

use crate::model::{Field, Language, Param, Route, Symbol, SymbolKind};
use crate::text::{
    brace_block_end, indent_block_end, indent_of, split_top_level, squash, LineIndex,
};
use regex::Regex;
use std::collections::BTreeMap;
use std::sync::LazyLock;

/// A parameter list that tolerates one level of nested parentheses.
const PARAMS: &str = r"((?:[^()]|\([^()]*\))*)";

macro_rules! re {
    ($name:ident, $pat:expr) => {
        static $name: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(&$pat.replace("PARAMS", PARAMS)).expect(stringify!($name)));
    };
}

#[derive(Debug, Default)]
pub struct Extracted {
    pub imports: Vec<String>,
    pub symbols: Vec<Symbol>,
    pub routes: Vec<Route>,
    pub concepts: BTreeMap<String, usize>,
    pub mounts: Vec<(String, String)>,
}

struct Ctx<'a> {
    text: &'a str,
    idx: LineIndex,
    lines: Vec<&'a str>,
    lang: Language,
}

impl<'a> Ctx<'a> {
    fn new(text: &'a str, lang: Language) -> Self {
        Ctx {
            text,
            idx: LineIndex::new(text),
            lines: text.lines().collect(),
            lang,
        }
    }

    fn line(&self, offset: usize) -> usize {
        self.idx.line_of(offset)
    }

    /// 1-based last line of the block that starts at `offset`.
    fn end_line(&self, offset: usize) -> usize {
        let line = self.line(offset);
        if self.lang.uses_indentation_blocks() {
            indent_block_end(&self.lines, line)
        } else {
            brace_block_end(self.text, offset)
                .map(|e| self.line(e.saturating_sub(1)))
                .unwrap_or(line)
        }
    }

    fn block_range(&self, offset: usize) -> (usize, usize) {
        let end = brace_block_end(self.text, offset).unwrap_or(offset);
        (offset, end)
    }
}

pub fn extract(lang: Language, text: &str) -> Extracted {
    let ctx = Ctx::new(text, lang);
    let mut out = match lang {
        Language::Python => python(&ctx),
        Language::JavaScript | Language::TypeScript => javascript(&ctx),
        Language::Go => go(&ctx),
        Language::Java | Language::CSharp => jvm_like(&ctx),
        Language::Kotlin => kotlin(&ctx),
        Language::C | Language::Cpp => c_like(&ctx),
        Language::Ruby => ruby(&ctx),
        Language::Php => php(&ctx),
        _ => Extracted::default(),
    };
    let inherit = out
        .symbols
        .iter()
        .filter(|s| !s.bases.is_empty() && s.kind != SymbolKind::Enum)
        .count();
    if inherit > 0 {
        out.concepts.insert("inheritance".into(), inherit);
    }
    out.concepts.retain(|_, v| *v > 0);
    out.imports.sort();
    out.imports.dedup();
    out
}

fn count(re: &Regex, text: &str) -> usize {
    re.find_iter(text).count()
}

fn sym(kind: SymbolKind, name: &str, line: usize, end_line: usize, signature: &str) -> Symbol {
    Symbol {
        kind,
        name: name.to_string(),
        parent: None,
        params: vec![],
        return_type: None,
        is_async: false,
        line,
        end_line,
        bases: vec![],
        fields: vec![],
        signature: squash(signature).chars().take(240).collect(),
    }
}

fn opt_str(m: Option<regex::Match>) -> Option<String> {
    m.map(|m| m.as_str().trim().to_string())
        .filter(|s| !s.is_empty())
}

fn split_names(s: &str) -> Vec<String> {
    s.split([',', ' '])
        .map(|p| p.trim().trim_start_matches(':').to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

/// Index of the innermost `(start, end, name)` range containing `offset`.
fn enclosing(ranges: &[(usize, usize, String)], offset: usize) -> Option<&(usize, usize, String)> {
    ranges
        .iter()
        .filter(|(s, e, _)| *s < offset && offset < *e)
        .min_by_key(|(s, e, _)| e - s)
}

const KEYWORDS: &[&str] = &[
    "if", "for", "while", "switch", "catch", "function", "return", "else", "do", "try", "with",
    "sizeof", "foreach", "using", "lock", "new", "typeof", "await", "yield", "elif", "match",
];

// ---------------------------------------------------------------------------
// Python
// ---------------------------------------------------------------------------

re!(
    PY_IMPORT,
    r"(?m)^[ \t]*(?:from[ \t]+([\w\.]+)[ \t]+import[ \t]+([^\n#]+)|import[ \t]+([\w\.]+(?:[ \t]*,[ \t]*[\w\.]+)*))"
);
re!(
    PY_DEF,
    r"(?m)^([ \t]*)(async[ \t]+)?def[ \t]+(\w+)[ \t]*\(PARAMS\)[ \t]*(?:->[ \t]*([^:\n]+))?[ \t]*:"
);
re!(
    PY_CLASS,
    r"(?m)^([ \t]*)class[ \t]+(\w+)[ \t]*(?:\(PARAMS\))?[ \t]*:"
);
re!(
    PY_ROUTE,
    r"(?m)^[ \t]*@(\w+)\.(route|get|post|put|delete|patch|api_route)\(\s*['\x22]([^'\x22]*)['\x22]PARAMS\)"
);
re!(
    PY_DJANGO,
    r#"\b(?:re_)?path\(\s*r?['"]([^'"]*)['"]\s*,\s*([\w\.]+)"#
);
re!(PY_METHODS, r"methods\s*=\s*\[([^\]]*)\]");
re!(
    PY_FIELD,
    r"^[ \t]+(\w+)[ \t]*:[ \t]*([^=\n#]+?)[ \t]*(?:=.*)?$"
);
re!(
    PY_ENUM_VARIANT,
    r#"^[ \t]+([A-Z_][A-Z0-9_]*)[ \t]*=[ \t]*(?:['"]([^'"]*)['"])?"#
);
re!(
    PY_SELF_ATTR,
    r"self\.(\w+)[ \t]*(?::[ \t]*([^=\n]+?))?[ \t]*=[ \t]*([^=\n][^\n]*)"
);
re!(PY_EVAL, r"\b(?:eval|exec|compile)\s*\(");
re!(
    PY_REFLECT,
    r"\b(?:getattr|setattr|hasattr|delattr|__getattr__|__setattr__|metaclass|importlib)\b"
);
re!(PY_EXCEPT, r"\bexcept\b|\braise\b");
re!(PY_NONE, r"\bNone\b");
re!(PY_ASYNC, r"\basync\s+def\b|\bawait\b|\basyncio\.");
re!(
    PY_THREADS,
    r"\bthreading\.|\bmultiprocessing\.|concurrent\.futures"
);
re!(PY_DECOR, r"(?m)^[ \t]*@\w+");
re!(PY_GLOBAL, r"(?m)^[ \t]*global[ \t]");
re!(PY_YIELD, r"\byield\b");
re!(PY_LAMBDA, r"\blambda\b");
re!(PY_ANY, r":\s*Any\b|->\s*Any\b");

fn python(ctx: &Ctx) -> Extracted {
    let t = ctx.text;
    let mut out = Extracted::default();

    for c in PY_IMPORT.captures_iter(t) {
        if let Some(module) = c.get(1) {
            let module = module.as_str();
            if module.chars().all(|ch| ch == '.') {
                for name in c[2].split(',') {
                    let name = name
                        .split_whitespace()
                        .next()
                        .unwrap_or("")
                        .trim_matches(|ch| ch == '(' || ch == ')');
                    if !name.is_empty() {
                        out.imports.push(format!("{module}{name}"));
                    }
                }
            } else {
                out.imports.push(module.to_string());
            }
        } else if let Some(list) = c.get(3) {
            for m in list.as_str().split(',') {
                out.imports.push(m.trim().to_string());
            }
        }
    }

    // Classes and defs, ordered by offset, with an indentation stack for parents.
    enum Item<'t> {
        Class(regex::Captures<'t>),
        Def(regex::Captures<'t>),
    }
    let mut items: Vec<(usize, Item)> = PY_CLASS
        .captures_iter(t)
        .map(|c| (c.get(0).unwrap().start(), Item::Class(c)))
        .chain(
            PY_DEF
                .captures_iter(t)
                .map(|c| (c.get(0).unwrap().start(), Item::Def(c))),
        )
        .collect();
    items.sort_by_key(|(o, _)| *o);

    let mut stack: Vec<(usize, String)> = vec![];
    for (offset, item) in items {
        let (indent_str, header_len) = match &item {
            Item::Class(c) | Item::Def(c) => (c[1].to_string(), c.get(0).unwrap().len()),
        };
        let indent = indent_of(&indent_str);
        while stack.last().is_some_and(|(i, _)| *i >= indent) {
            stack.pop();
        }
        let parent = stack.last().map(|(_, n)| n.clone());
        let line = ctx.line(offset + indent_str.len());
        let end = ctx.end_line(offset);
        let signature = &t[offset..offset + header_len];
        match item {
            Item::Class(c) => {
                let name = c[2].to_string();
                let mut s = sym(SymbolKind::Class, &name, line, end, signature);
                s.parent = parent;
                s.bases = c
                    .get(3)
                    .map(|b| split_top_level(b.as_str()))
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|b| b != "object" && !b.contains('='))
                    .collect();
                let is_enum = s.bases.iter().any(|b| b.ends_with("Enum"));
                if is_enum {
                    s.kind = SymbolKind::Enum;
                }
                python_class_fields(ctx, &mut s, indent, is_enum);
                stack.push((indent, name));
                out.symbols.push(s);
            }
            Item::Def(c) => {
                let name = c[3].to_string();
                let kind = if parent.is_some() {
                    SymbolKind::Method
                } else {
                    SymbolKind::Function
                };
                let mut s = sym(kind, &name, line, end, signature);
                s.is_async = c.get(2).is_some();
                s.params = parse_params(Language::Python, &c[4]);
                s.return_type = opt_str(c.get(5));
                s.parent = parent;
                // Nested functions inside functions are folded into their parent.
                let nested_in_fn = stack.last().is_some_and(|(_, n)| {
                    out.symbols.iter().rev().any(|p| {
                        &p.name == n && matches!(p.kind, SymbolKind::Function | SymbolKind::Method)
                    })
                });
                if !nested_in_fn {
                    out.symbols.push(s);
                }
                stack.push((indent, name));
            }
        }
    }

    // Routes: decorator followed by a def.
    for c in PY_ROUTE.captures_iter(t) {
        let end = c.get(0).unwrap().end();
        let handler = PY_DEF.captures(&t[end..]).map(|d| d[3].to_string());
        let verb = c[2].to_ascii_lowercase();
        let methods: Vec<String> = if verb == "route" || verb == "api_route" {
            PY_METHODS
                .captures(&c[4])
                .map(|m| {
                    m[1].split(',')
                        .map(|x| {
                            x.trim()
                                .trim_matches(|ch| ch == '\'' || ch == '"')
                                .to_ascii_uppercase()
                        })
                        .filter(|x| !x.is_empty())
                        .collect()
                })
                .unwrap_or_else(|| vec!["GET".into()])
        } else {
            vec![verb.to_ascii_uppercase()]
        };
        let framework = if verb == "route" { "flask" } else { "fastapi" };
        for m in methods {
            out.routes.push(Route {
                method: m,
                path: c[3].to_string(),
                handler: handler.clone(),
                framework: framework.into(),
                line: ctx.line(c.get(0).unwrap().start()),
                receiver: None,
            });
        }
    }
    if t.contains("urlpatterns") {
        for c in PY_DJANGO.captures_iter(t) {
            out.routes.push(Route {
                method: "ANY".into(),
                path: format!("/{}", c[1].trim_start_matches('^').trim_end_matches('$')),
                handler: c[2].rsplit('.').next().map(str::to_string),
                framework: "django".into(),
                line: ctx.line(c.get(0).unwrap().start()),
                receiver: None,
            });
        }
    }

    let c = &mut out.concepts;
    c.insert("dynamic_eval".into(), count(&PY_EVAL, t));
    c.insert("reflection".into(), count(&PY_REFLECT, t));
    c.insert("exceptions".into(), count(&PY_EXCEPT, t));
    c.insert("null_values".into(), count(&PY_NONE, t));
    c.insert("async".into(), count(&PY_ASYNC, t));
    c.insert("threads".into(), count(&PY_THREADS, t));
    c.insert("decorators".into(), count(&PY_DECOR, t));
    c.insert("global_state".into(), count(&PY_GLOBAL, t));
    c.insert("generators".into(), count(&PY_YIELD, t));
    c.insert("closures".into(), count(&PY_LAMBDA, t));
    c.insert("any_type".into(), count(&PY_ANY, t));
    out
}

fn python_class_fields(ctx: &Ctx, s: &mut Symbol, class_indent: usize, is_enum: bool) {
    let mut body_indent: Option<usize> = None;
    for l in ctx.lines.iter().take(s.end_line).skip(s.line) {
        if l.trim().is_empty() || l.trim_start().starts_with('#') {
            continue;
        }
        let ind = indent_of(l);
        if ind <= class_indent {
            break;
        }
        let bi = *body_indent.get_or_insert(ind);
        if ind != bi {
            continue;
        }
        let trimmed = l.trim_start();
        if trimmed.starts_with("def ") || trimmed.starts_with("async ") || trimmed.starts_with('@')
        {
            continue;
        }
        if is_enum {
            if let Some(c) = PY_ENUM_VARIANT.captures(l) {
                s.fields.push(Field {
                    name: c[1].to_string(),
                    ty: opt_str(c.get(2)),
                    optional: false,
                });
            }
        } else if let Some(c) = PY_FIELD.captures(l) {
            let ty = c[2].trim().to_string();
            let optional = ty.starts_with("Optional") || ty.contains("None");
            s.fields.push(Field {
                name: c[1].to_string(),
                ty: Some(ty),
                optional,
            });
        }
    }
    if is_enum {
        return;
    }
    // Attributes assigned in methods (`self.x = ...`).
    let body = ctx
        .lines
        .iter()
        .take(s.end_line)
        .skip(s.line)
        .copied()
        .collect::<Vec<_>>()
        .join("\n");
    for c in PY_SELF_ATTR.captures_iter(&body) {
        let name = c[1].to_string();
        if !s.fields.iter().any(|f| f.name == name) {
            let ty = opt_str(c.get(2)).or_else(|| infer_literal(&format!("x = {}", c[3].trim())));
            s.fields.push(Field {
                name,
                ty,
                optional: false,
            });
        }
    }
}

// ---------------------------------------------------------------------------
// JavaScript / TypeScript
// ---------------------------------------------------------------------------

re!(
    JS_IMPORT,
    r#"(?m)^[ \t]*(?:import|export)[ \t]+(?:type[ \t]+)?(?:[\w*{}\s,]+?[ \t]+from[ \t]+)?['"]([^'"]+)['"]"#
);
re!(JS_REQUIRE, r#"\brequire\(\s*['"]([^'"]+)['"]\s*\)"#);
re!(JS_DYN_IMPORT, r#"\bimport\(\s*['"]([^'"]+)['"]\s*\)"#);
re!(
    JS_FUNC,
    r"(?m)^[ \t]*(?:export[ \t]+)?(?:default[ \t]+)?(async[ \t]+)?function[ \t]*\*?[ \t]*(\w+)[ \t]*(?:<[^>()]*>)?[ \t]*\(PARAMS\)[ \t]*(?::[ \t]*([^{\n]+?))?[ \t]*\{"
);
re!(
    JS_ARROW,
    r"(?m)^[ \t]*(?:export[ \t]+)?(?:const|let|var)[ \t]+(\w+)[ \t]*(?::[^=\n]+)?=[ \t]*(async[ \t]+)?(?:\(PARAMS\)|(\w+))[ \t]*(?::[ \t]*([^=\n]+?))?[ \t]*=>"
);
re!(
    JS_CLASS,
    r"(?m)^[ \t]*(?:export[ \t]+)?(?:default[ \t]+)?(?:abstract[ \t]+)?class[ \t]+(\w+)(?:<[^>{]*>)?(?:[ \t]+extends[ \t]+([\w\.]+)(?:<[^>{]*>)?)?(?:[ \t]+implements[ \t]+([\w\., <>]+?))?[ \t]*\{"
);
re!(
    JS_METHOD,
    r"(?m)^[ \t]+((?:(?:public|private|protected|static|readonly|override|abstract|get|set)[ \t]+)*)(async[ \t]+)?\*?(\w+)[ \t]*(?:<[^>()]*>)?[ \t]*\(PARAMS\)[ \t]*(?::[ \t]*([^{\n;]+?))?[ \t]*\{"
);
re!(
    JS_CLASS_FIELD,
    r"(?m)^[ \t]+(?:(?:public|private|protected|readonly|static|declare)[ \t]+)*(\w+)([?!])?[ \t]*:[ \t]*([^;=\n]+?)[ \t]*(?:=[^;\n]*)?;?[ \t]*$"
);
re!(JS_THIS_FIELD, r"this\.(\w+)[ \t]*=[^=]");
re!(
    TS_INTERFACE,
    r"(?m)^[ \t]*(?:export[ \t]+)?(?:declare[ \t]+)?interface[ \t]+(\w+)(?:<[^>{]*>)?(?:[ \t]+extends[ \t]+([\w\., <>]+?))?[ \t]*\{"
);
re!(
    TS_TYPE_OBJ,
    r"(?m)^[ \t]*(?:export[ \t]+)?type[ \t]+(\w+)(?:<[^>=]*>)?[ \t]*=[ \t]*\{"
);
re!(
    TS_FIELD,
    r"^[ \t]*(?:readonly[ \t]+)?['\x22]?(\w+)['\x22]?(\?)?[ \t]*:[ \t]*([^;\n]+?)[ \t]*[;,]?[ \t]*$"
);
re!(
    TS_ENUM,
    r"(?m)^[ \t]*(?:export[ \t]+)?(?:declare[ \t]+)?(?:const[ \t]+)?enum[ \t]+(\w+)[ \t]*\{([^}]*)\}"
);
re!(
    JS_ROUTE,
    r"\b(\w+)\.(get|post|put|delete|patch|all|options|head)\(\s*['\x22`]([^'\x22`]+)['\x22`]\s*,PARAMS\)"
);
re!(
    JS_MOUNT,
    r#"\b\w+\.use\(\s*['"`](/[^'"`]*)['"`]\s*,\s*(?:[\w\.]+\s*,\s*)*(\w+)\s*\)"#
);
re!(
    NEST_ROUTE,
    r"@(Get|Post|Put|Delete|Patch|All)\(\s*(?:['\x22`]([^'\x22`]*)['\x22`])?\s*\)"
);
re!(
    NEST_CONTROLLER,
    r"@Controller\(\s*(?:['\x22`]([^'\x22`]*)['\x22`])?"
);
re!(JS_EVAL, r"\beval\s*\(|\bnew\s+Function\s*\(");
re!(JS_ANY, r":\s*any\b|<any>|as\s+any\b");
re!(JS_TRY, r"\bcatch\s*[\({]|\bthrow\s");
re!(JS_NULL, r"\bnull\b|\bundefined\b");
re!(JS_ASYNC, r"\basync\b|\bawait\b|\bPromise\b|\.then\(");
re!(
    JS_PROTO,
    r"\.prototype\b|Object\.defineProperty|Reflect\.|\bProxy\("
);
re!(JS_GLOBAL, r"(?m)^(?:let|var)[ \t]+\w+");
re!(JS_ARROWS, r"=>");
re!(JS_DECOR, r"(?m)^[ \t]*@\w+");

fn javascript(ctx: &Ctx) -> Extracted {
    let t = ctx.text;
    let mut out = Extracted::default();
    for re in [&*JS_IMPORT, &*JS_REQUIRE, &*JS_DYN_IMPORT] {
        for c in re.captures_iter(t) {
            out.imports.push(c[1].to_string());
        }
    }

    // Classes first so methods can find their parent.
    let mut class_ranges: Vec<(usize, usize, String)> = vec![];
    for c in JS_CLASS.captures_iter(t) {
        let m = c.get(0).unwrap();
        let (start, end) = ctx.block_range(m.start());
        let name = c[1].to_string();
        let mut s = sym(
            SymbolKind::Class,
            &name,
            ctx.line(m.start()),
            ctx.line(end.saturating_sub(1)),
            m.as_str(),
        );
        if let Some(b) = c.get(2) {
            s.bases.push(b.as_str().to_string());
        }
        if let Some(i) = c.get(3) {
            s.bases.extend(split_top_level(i.as_str()));
        }
        class_ranges.push((start, end, name));
        out.symbols.push(s);
    }

    // Methods inside class bodies (skip ones nested in another method).
    let mut method_ranges: Vec<(usize, usize)> = vec![];
    for c in JS_METHOD.captures_iter(t) {
        let m = c.get(0).unwrap();
        let name = &c[3];
        if KEYWORDS.contains(&name) {
            continue;
        }
        let Some((_, _, class)) = enclosing(&class_ranges, m.start()) else {
            continue;
        };
        if method_ranges
            .iter()
            .any(|(s, e)| *s < m.start() && m.start() < *e)
        {
            continue;
        }
        let (start, end) = ctx.block_range(m.start());
        method_ranges.push((start, end));
        let mut s = sym(
            SymbolKind::Method,
            name,
            ctx.line(m.start()),
            ctx.line(end.saturating_sub(1)),
            m.as_str(),
        );
        s.parent = Some(class.clone());
        s.is_async = c.get(2).is_some();
        s.params = parse_params(ctx.lang, &c[4]);
        s.return_type = opt_str(c.get(5));
        out.symbols.push(s);
    }

    // Class fields (outside method bodies) and `this.x =` assignments.
    for (cs, ce, class) in &class_ranges {
        let mut fields: Vec<Field> = vec![];
        for c in JS_CLASS_FIELD.captures_iter(&t[*cs..*ce]) {
            let off = cs + c.get(0).unwrap().start();
            if method_ranges.iter().any(|(s, e)| *s < off && off < *e) || off == *cs {
                continue;
            }
            fields.push(Field {
                name: c[1].to_string(),
                ty: Some(c[3].trim().to_string()),
                optional: c.get(2).is_some_and(|m| m.as_str() == "?"),
            });
        }
        for c in JS_THIS_FIELD.captures_iter(&t[*cs..*ce]) {
            if !fields.iter().any(|f| f.name == c[1]) {
                fields.push(Field {
                    name: c[1].to_string(),
                    ty: None,
                    optional: false,
                });
            }
        }
        if let Some(s) = out
            .symbols
            .iter_mut()
            .find(|s| s.kind == SymbolKind::Class && &s.name == class)
        {
            s.fields = fields;
        }
    }

    let in_class = |off: usize| class_ranges.iter().any(|(s, e, _)| *s < off && off < *e);

    for c in JS_FUNC.captures_iter(t) {
        let m = c.get(0).unwrap();
        if in_class(m.start())
            || method_ranges
                .iter()
                .any(|(s, e)| *s < m.start() && m.start() < *e)
        {
            continue;
        }
        let mut s = sym(
            SymbolKind::Function,
            &c[2],
            ctx.line(m.start()),
            ctx.end_line(m.start()),
            m.as_str(),
        );
        s.is_async = c.get(1).is_some();
        s.params = parse_params(ctx.lang, &c[3]);
        s.return_type = opt_str(c.get(4));
        out.symbols.push(s);
    }
    for c in JS_ARROW.captures_iter(t) {
        let m = c.get(0).unwrap();
        if in_class(m.start()) {
            continue;
        }
        // Only top-level (unindented) arrow functions are treated as functions.
        if !m.as_str().starts_with(|ch: char| !ch.is_whitespace()) {
            continue;
        }
        let rest = &t[m.end()..];
        let body_is_block = rest.trim_start().starts_with('{');
        let end = if body_is_block {
            ctx.end_line(m.end())
        } else {
            ctx.line(m.end())
        };
        let mut s = sym(
            SymbolKind::Function,
            &c[1],
            ctx.line(m.start()),
            end,
            m.as_str(),
        );
        s.is_async = c.get(2).is_some();
        let raw = c.get(3).or(c.get(4)).map(|m| m.as_str()).unwrap_or("");
        s.params = parse_params(ctx.lang, raw);
        s.return_type = opt_str(c.get(5));
        out.symbols.push(s);
    }

    // Interfaces / object type aliases -> data structs.
    for (re, kind) in [
        (&*TS_INTERFACE, SymbolKind::Interface),
        (&*TS_TYPE_OBJ, SymbolKind::Struct),
    ] {
        for c in re.captures_iter(t) {
            let m = c.get(0).unwrap();
            let (start, end) = ctx.block_range(m.start());
            let mut s = sym(
                kind,
                &c[1],
                ctx.line(m.start()),
                ctx.line(end.saturating_sub(1)),
                m.as_str(),
            );
            if kind == SymbolKind::Interface {
                if let Some(b) = c.get(2) {
                    s.bases = split_top_level(b.as_str());
                }
            }
            s.fields = brace_body_fields(&t[start..end], &TS_FIELD);
            out.symbols.push(s);
        }
    }
    for c in TS_ENUM.captures_iter(t) {
        let m = c.get(0).unwrap();
        let mut s = sym(
            SymbolKind::Enum,
            &c[1],
            ctx.line(m.start()),
            ctx.line(m.end()),
            m.as_str(),
        );
        s.fields = enum_variants(&c[2]);
        out.symbols.push(s);
    }

    // Express / Koa / Fastify style routes.
    for c in JS_ROUTE.captures_iter(t) {
        let recv = c[1].to_ascii_lowercase();
        let looks_like_server = [
            "app", "router", "server", "api", "routes", "r", "fastify", "route",
        ]
        .contains(&recv.as_str())
            || recv.ends_with("router")
            || recv.ends_with("app");
        if !looks_like_server || !c[3].starts_with('/') && &c[3] != "*" {
            continue;
        }
        let args = split_top_level(&c[4]);
        let handler = args
            .last()
            .map(|a| a.trim())
            .filter(|a| {
                a.chars()
                    .all(|ch| ch.is_alphanumeric() || ch == '_' || ch == '.')
            })
            .and_then(|a| a.rsplit('.').next().map(str::to_string));
        out.routes.push(Route {
            method: c[2].to_ascii_uppercase(),
            path: c[3].to_string(),
            handler,
            framework: "express".into(),
            line: ctx.line(c.get(0).unwrap().start()),
            receiver: Some(c[1].to_string()),
        });
    }
    for c in JS_MOUNT.captures_iter(t) {
        out.mounts.push((c[1].to_string(), c[2].to_string()));
    }
    // NestJS decorators.
    let prefix = NEST_CONTROLLER
        .captures(t)
        .and_then(|c| opt_str(c.get(1)))
        .unwrap_or_default();
    for c in NEST_ROUTE.captures_iter(t) {
        let end = c.get(0).unwrap().end();
        let handler = JS_METHOD.captures(&t[end..]).map(|m| m[3].to_string());
        let sub = opt_str(c.get(2)).unwrap_or_default();
        let path = format!("/{}/{}", prefix.trim_matches('/'), sub.trim_matches('/'));
        out.routes.push(Route {
            method: c[1].to_ascii_uppercase(),
            path: normalize_slashes(&path),
            handler,
            framework: "nestjs".into(),
            line: ctx.line(c.get(0).unwrap().start()),
            receiver: None,
        });
    }

    let c = &mut out.concepts;
    c.insert("dynamic_eval".into(), count(&JS_EVAL, t));
    c.insert("any_type".into(), count(&JS_ANY, t));
    c.insert("exceptions".into(), count(&JS_TRY, t));
    c.insert("null_values".into(), count(&JS_NULL, t));
    c.insert("async".into(), count(&JS_ASYNC, t));
    c.insert("reflection".into(), count(&JS_PROTO, t));
    c.insert("global_state".into(), count(&JS_GLOBAL, t));
    c.insert("closures".into(), count(&JS_ARROWS, t));
    c.insert("decorators".into(), count(&JS_DECOR, t));
    out
}

fn normalize_slashes(p: &str) -> String {
    let mut s = String::from("/");
    s.push_str(
        &p.split('/')
            .filter(|x| !x.is_empty())
            .collect::<Vec<_>>()
            .join("/"),
    );
    s
}

/// Fields from the first indentation level of a `{ ... }` body.
fn brace_body_fields(body: &str, field_re: &Regex) -> Vec<Field> {
    let inner = body
        .trim()
        .trim_start_matches(|c| c != '{')
        .trim_start_matches('{');
    let inner = inner.strip_suffix('}').unwrap_or(inner);
    let mut level: Option<usize> = None;
    let mut out = vec![];
    for l in inner.lines() {
        if l.trim().is_empty()
            || l.trim_start().starts_with("//")
            || l.trim_start().starts_with('*')
            || l.trim_start().starts_with("/*")
        {
            continue;
        }
        let ind = indent_of(l);
        let lv = *level.get_or_insert(ind);
        if ind != lv {
            continue;
        }
        if let Some(c) = field_re.captures(l) {
            let ty = c[3].trim().trim_end_matches(&[';', ','][..]).to_string();
            let optional = c.get(2).is_some_and(|m| m.as_str() == "?")
                || ty.contains("| null")
                || ty.contains("| undefined");
            out.push(Field {
                name: c[1].to_string(),
                ty: Some(ty),
                optional,
            });
        }
    }
    out
}

fn enum_variants(body: &str) -> Vec<Field> {
    body.split([',', ';'])
        .filter_map(|p| {
            let p = p.trim();
            let name: String = p
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            // `Admin = 'admin'` -> wire value "admin"
            let value = p
                .split_once('=')
                .map(|(_, v)| v.trim())
                .filter(|v| v.starts_with(['\'', '"', '`']))
                .map(|v| {
                    v.trim_matches(|c| c == '\'' || c == '"' || c == '`')
                        .to_string()
                });
            (!name.is_empty() && !p.starts_with("//")).then_some(Field {
                name,
                ty: value,
                optional: false,
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Go
// ---------------------------------------------------------------------------

re!(GO_IMPORT, r#"(?m)^import[ \t]+(?:[\w\.]+[ \t]+)?"([^"]+)""#);
re!(GO_IMPORT_BLOCK, r"(?s)\bimport[ \t]*\((.*?)\)");
re!(GO_IMPORT_LINE, r#""([^"]+)""#);
re!(
    GO_FUNC,
    r"(?m)^func[ \t]*(?:\([ \t]*(?:\w+[ \t]+)?\*?[ \t]*(\w+)(?:\[[^\]]*\])?[ \t]*\)[ \t]*)?(\w+)(?:\[[^\]]*\])?[ \t]*\(PARAMS\)[ \t]*([^{\n]*)\{"
);
re!(
    GO_STRUCT,
    r"(?m)^type[ \t]+(\w+)(?:\[[^\]]*\])?[ \t]+struct[ \t]*\{"
);
re!(
    GO_IFACE,
    r"(?m)^type[ \t]+(\w+)(?:\[[^\]]*\])?[ \t]+interface[ \t]*\{"
);
re!(
    GO_FIELD,
    r"^[ \t]*([A-Za-z_]\w*(?:[ \t]*,[ \t]*[A-Za-z_]\w*)*)[ \t]+([^`/\n]+?)[ \t]*(?:`.*)?(?://.*)?$"
);
re!(GO_EMBED, r"^[ \t]*\*?([A-Za-z_][\w\.]*)[ \t]*(?://.*)?$");
re!(
    GO_HANDLE,
    r#"\.(?:HandleFunc|Handle)\(\s*"([^"]+)"\s*,\s*([\w\.]+)"#
);
re!(
    GO_ROUTER,
    r#"\b\w+\.(GET|POST|PUT|DELETE|PATCH|Get|Post|Put|Delete|Patch|Any)\(\s*"(/[^"]*)"\s*,\s*([\w\.]*)"#
);
re!(GO_JSON_TAG, r#"json:"([^",]*)"#);
re!(GO_GOROUTINE, r"(?m)\bgo[ \t]+(?:func\b|[\w\.]+\()");
re!(GO_CHAN, r"\bchan\b|<-");
re!(GO_REFLECT, r"\breflect\.");
re!(GO_UNSAFE, r"\bunsafe\.");
re!(GO_ANY, r"interface\{\}|\bany\b");
re!(GO_NIL, r"\bnil\b");
re!(GO_ERR, r"err\s*!=\s*nil|\bpanic\(");
re!(GO_DEFER, r"\bdefer\b");
re!(GO_MUTEX, r"sync\.(?:Mutex|RWMutex|WaitGroup|Once)");

fn go(ctx: &Ctx) -> Extracted {
    let t = ctx.text;
    let mut out = Extracted::default();
    for c in GO_IMPORT.captures_iter(t) {
        out.imports.push(c[1].to_string());
    }
    for b in GO_IMPORT_BLOCK.captures_iter(t) {
        for c in GO_IMPORT_LINE.captures_iter(&b[1]) {
            out.imports.push(c[1].to_string());
        }
    }
    for c in GO_FUNC.captures_iter(t) {
        let m = c.get(0).unwrap();
        let receiver = opt_str(c.get(1));
        let kind = if receiver.is_some() {
            SymbolKind::Method
        } else {
            SymbolKind::Function
        };
        let mut s = sym(
            kind,
            &c[2],
            ctx.line(m.start()),
            ctx.end_line(m.start()),
            m.as_str().trim_end_matches('{'),
        );
        s.parent = receiver;
        s.params = parse_params(Language::Go, &c[3]);
        s.return_type = opt_str(c.get(4));
        out.symbols.push(s);
    }
    for (re, kind) in [
        (&*GO_STRUCT, SymbolKind::Struct),
        (&*GO_IFACE, SymbolKind::Interface),
    ] {
        for c in re.captures_iter(t) {
            let m = c.get(0).unwrap();
            let (start, end) = ctx.block_range(m.start());
            let mut s = sym(
                kind,
                &c[1],
                ctx.line(m.start()),
                ctx.line(end.saturating_sub(1)),
                m.as_str(),
            );
            if kind == SymbolKind::Struct {
                let body = &t[start..end];
                let inner = body.split_once('{').map(|x| x.1).unwrap_or("");
                for l in inner.lines() {
                    let l = l.trim_end_matches('}');
                    if l.trim().is_empty() || l.trim_start().starts_with("//") {
                        continue;
                    }
                    if let Some(f) = GO_FIELD.captures(l) {
                        let ty = f[2].to_string();
                        // The json tag is the wire name; `json:"-"` means not serialized.
                        let tag = GO_JSON_TAG.captures(l).map(|t| t[1].to_string());
                        if tag.as_deref() == Some("-") {
                            continue;
                        }
                        let names: Vec<&str> = f[1].split(',').map(str::trim).collect();
                        for name in &names {
                            let name = match (&tag, names.len()) {
                                (Some(t), 1) if !t.is_empty() => t.clone(),
                                _ => name.to_string(),
                            };
                            s.fields.push(Field {
                                name,
                                ty: Some(ty.clone()),
                                optional: ty.starts_with('*'),
                            });
                        }
                    } else if let Some(e) = GO_EMBED.captures(l) {
                        s.bases.push(e[1].to_string());
                    }
                }
            }
            out.symbols.push(s);
        }
    }
    for c in GO_HANDLE.captures_iter(t) {
        let raw = c[1].to_string();
        let (method, path) = match raw.split_once(' ') {
            Some((m, p)) if m.chars().all(|ch| ch.is_ascii_uppercase()) => {
                (m.to_string(), p.to_string())
            }
            _ => ("ANY".to_string(), raw),
        };
        out.routes.push(Route {
            method,
            path,
            handler: c[2].rsplit('.').next().map(str::to_string),
            framework: "net/http".into(),
            line: ctx.line(c.get(0).unwrap().start()),
            receiver: None,
        });
    }
    for c in GO_ROUTER.captures_iter(t) {
        // `r.GET("/x", func(c *gin.Context) {...})` has an inline handler.
        let handler = Some(c[3].to_string())
            .filter(|h| !h.is_empty() && h != "func")
            .and_then(|h| h.rsplit('.').next().map(str::to_string));
        out.routes.push(Route {
            method: c[1].to_ascii_uppercase(),
            path: c[2].to_string(),
            handler,
            framework: "gin/echo/chi".into(),
            line: ctx.line(c.get(0).unwrap().start()),
            receiver: None,
        });
    }
    let c = &mut out.concepts;
    c.insert("goroutines".into(), count(&GO_GOROUTINE, t));
    c.insert("channels".into(), count(&GO_CHAN, t));
    c.insert("reflection".into(), count(&GO_REFLECT, t));
    c.insert("unsafe_pointers".into(), count(&GO_UNSAFE, t));
    c.insert("any_type".into(), count(&GO_ANY, t));
    c.insert("null_values".into(), count(&GO_NIL, t));
    c.insert("error_values".into(), count(&GO_ERR, t));
    c.insert("defer".into(), count(&GO_DEFER, t));
    c.insert("threads".into(), count(&GO_MUTEX, t));
    out
}

// ---------------------------------------------------------------------------
// Java / C#
// ---------------------------------------------------------------------------

re!(
    JAVA_IMPORT,
    r"(?m)^[ \t]*import[ \t]+(?:static[ \t]+)?([\w\.\*]+)[ \t]*;"
);
re!(
    CS_USING,
    r"(?m)^[ \t]*using[ \t]+(?:static[ \t]+)?([\w\.]+)[ \t]*;"
);
re!(
    JAVA_TYPE,
    r"(?m)^[ \t]*(?:(?:public|private|protected|internal|abstract|static|final|sealed|partial|readonly|non-sealed)[ \t]+)*(class|interface|enum|record|struct)[ \t]+(\w+)(?:<[^>{]*>)?([^{;]*?)\s*(?:\{|;)"
);
re!(
    JAVA_METHOD,
    r"(?m)^[ \t]*((?:(?:public|private|protected|internal|static|final|abstract|synchronized|async|override|virtual|default|native|sealed|extern|unsafe|new)[ \t]+)+)([\w<>\[\],\.\? \t]+?)[ \t]+(\w+)[ \t]*(?:<[^>()]*>)?[ \t]*\(PARAMS\)\s*(?:throws[ \t]+[\w\., \t]+)?\s*(?:where[^{;]*?)?\s*(?:\{|=>)"
);
re!(
    JAVA_CTOR,
    r"(?m)^[ \t]*(?:public|private|protected|internal)[ \t]+(\w+)[ \t]*\(PARAMS\)\s*(?:throws[^{;]*)?(?::[^{;]*)?\s*\{"
);
re!(
    JAVA_FIELD,
    r"(?m)^[ \t]*((?:(?:private|public|protected|internal|final|readonly|static|volatile|transient|required)[ \t]+)+)([\w<>\[\],\.\? ]+?)[ \t]+(\w+)[ \t]*(?:=[^;\n]*)?;"
);
re!(
    CS_PROP,
    r"(?m)^[ \t]*(?:public|private|protected|internal)[ \t]+(?:(?:virtual|required|override|static)[ \t]+)*([\w<>\[\],\.\?]+)[ \t]+(\w+)[ \t]*\{[ \t]*(?:get|init|set)"
);
re!(
    SPRING_ROUTE,
    r#"@(Get|Post|Put|Delete|Patch|Request)Mapping(?:\(\s*(?:value\s*=\s*|path\s*=\s*)?\{?\s*"([^"]*)"[^)]*\))?"#
);
re!(
    ASPNET_ROUTE,
    r#"\[Http(Get|Post|Put|Delete|Patch)(?:\(\s*"([^"]*)"\s*\))?\]"#
);
re!(ASPNET_PREFIX, r#"\[Route\(\s*"([^"]*)"\s*\)\]"#);
re!(
    JVM_REFLECT,
    r"Class\.forName|\.getDeclared\w+\(|\.getMethod\(|typeof\(|\.GetType\(\)|Activator\.CreateInstance|\bdynamic\b"
);
re!(JVM_TRY, r"\bcatch\s*\(|\bthrows?\b");
re!(JVM_NULL, r"\bnull\b");
re!(
    JVM_ASYNC,
    r"CompletableFuture|\basync\b|\bawait\b|\bTask<|\bMono<|\bFlux<|@Async"
);
re!(
    JVM_THREADS,
    r"\bThread\b|ExecutorService|\bsynchronized\b|\block\s*\(|ConcurrentHashMap"
);
re!(JVM_ANNOT, r"(?m)^[ \t]*@\w+|^[ \t]*\[\w+");
re!(JVM_STREAMS, r"\.stream\(\)|\.Where\(|\.Select\(");
re!(JVM_OBJECT, r"\bObject\b|\bobject\b");

fn jvm_like(ctx: &Ctx) -> Extracted {
    let t = ctx.text;
    let lang = ctx.lang;
    let mut out = Extracted::default();
    let import_re = if lang == Language::CSharp {
        &*CS_USING
    } else {
        &*JAVA_IMPORT
    };
    for c in import_re.captures_iter(t) {
        out.imports.push(c[1].to_string());
    }

    let mut class_ranges: Vec<(usize, usize, String)> = vec![];
    for c in JAVA_TYPE.captures_iter(t) {
        let m = c.get(0).unwrap();
        let (start, end) = ctx.block_range(m.start());
        let name = c[2].to_string();
        let kind = match &c[1] {
            "interface" => SymbolKind::Interface,
            "enum" => SymbolKind::Enum,
            "struct" | "record" => SymbolKind::Struct,
            _ => SymbolKind::Class,
        };
        let mut s = sym(
            kind,
            &name,
            ctx.line(m.start()),
            ctx.line(end.saturating_sub(1)),
            m.as_str(),
        );
        let rest_owned = squash(&c[3]);
        let rest = rest_owned.trim();
        // Java record / C# primary-constructor components: record Foo(String a, int b)
        let rest = if let Some(inner) = rest.strip_prefix('(') {
            let (params, tail) = inner.split_once(')').unwrap_or((inner, ""));
            for p in parse_params(lang, params) {
                s.fields.push(Field {
                    name: p.name,
                    ty: p.ty,
                    optional: false,
                });
            }
            tail.trim().to_string()
        } else {
            rest.to_string()
        };
        let rest = rest.replace("extends", ",").replace("implements", ",");
        let rest = rest.trim_start_matches(':');
        s.bases = split_top_level(rest)
            .into_iter()
            .map(|b| b.trim().to_string())
            .filter(|b| !b.is_empty())
            .collect();
        if kind == SymbolKind::Enum {
            let body = &t[start..end];
            let inner = body.split_once('{').map(|x| x.1).unwrap_or("");
            let head = inner.split(';').next().unwrap_or("").trim_end_matches('}');
            let head: String = {
                // Drop constructor args: FOO("x", 1) -> FOO
                let mut depth = 0;
                head.chars()
                    .filter(|ch| {
                        match ch {
                            '(' => depth += 1,
                            ')' => {
                                depth -= 1;
                                return false;
                            }
                            _ => {}
                        }
                        depth == 0
                    })
                    .collect()
            };
            s.fields = enum_variants(&head);
        }
        class_ranges.push((start, end, name));
        out.symbols.push(s);
    }

    let mut method_ranges: Vec<(usize, usize)> = vec![];
    for c in JAVA_METHOD.captures_iter(t) {
        let m = c.get(0).unwrap();
        let ret = c[2].trim();
        let name = &c[3];
        if KEYWORDS.contains(&name) || ret == "new" || ret == "return" || ret == "else" {
            continue;
        }
        let parent = enclosing(&class_ranges, m.start()).map(|r| r.2.clone());
        let (start, end) = ctx.block_range(m.start());
        method_ranges.push((start, end));
        let kind = if parent.is_some() {
            SymbolKind::Method
        } else {
            SymbolKind::Function
        };
        let mut s = sym(
            kind,
            name,
            ctx.line(m.start()),
            ctx.line(end.saturating_sub(1)),
            m.as_str(),
        );
        s.parent = parent;
        s.is_async = c[1].contains("async")
            || ret.starts_with("Task")
            || ret.starts_with("CompletableFuture")
            || ret.starts_with("Mono");
        s.params = parse_params(lang, &c[4]);
        s.return_type = Some(ret.to_string()).filter(|r| r != "void");
        if c[1].contains("static") {
            s.signature = format!("static {}", s.signature);
        }
        out.symbols.push(s);
    }
    for c in JAVA_CTOR.captures_iter(t) {
        let m = c.get(0).unwrap();
        let Some((_, _, class)) = enclosing(&class_ranges, m.start()) else {
            continue;
        };
        if &c[1] != class {
            continue;
        }
        let (start, end) = ctx.block_range(m.start());
        method_ranges.push((start, end));
        let mut s = sym(
            SymbolKind::Method,
            "constructor",
            ctx.line(m.start()),
            ctx.line(end.saturating_sub(1)),
            m.as_str(),
        );
        s.parent = Some(class.clone());
        s.params = parse_params(lang, &c[2]);
        out.symbols.push(s);
    }

    // Fields and C# properties (outside methods).
    let add_field = |out: &mut Extracted, off: usize, name: &str, ty: &str| {
        if method_ranges.iter().any(|(s, e)| *s < off && off < *e) {
            return;
        }
        let Some((_, _, class)) = enclosing(&class_ranges, off) else {
            return;
        };
        if let Some(s) = out
            .symbols
            .iter_mut()
            .find(|s| &s.name == class && s.kind != SymbolKind::Method)
        {
            if !s.fields.iter().any(|f| f.name == name) {
                s.fields.push(Field {
                    name: name.to_string(),
                    ty: Some(ty.trim().to_string()),
                    optional: ty.ends_with('?') || ty.starts_with("Optional"),
                });
            }
        }
    };
    for c in JAVA_FIELD.captures_iter(t) {
        if c[1].contains("static") {
            continue;
        }
        add_field(&mut out, c.get(0).unwrap().start(), &c[3], &c[2]);
    }
    if lang == Language::CSharp {
        for c in CS_PROP.captures_iter(t) {
            add_field(&mut out, c.get(0).unwrap().start(), &c[2], &c[1]);
        }
    }

    // Spring routes.
    let first_class = class_ranges.first().map(|r| r.0).unwrap_or(usize::MAX);
    let mut prefix = String::new();
    for c in SPRING_ROUTE.captures_iter(t) {
        let m = c.get(0).unwrap();
        let path = opt_str(c.get(2)).unwrap_or_default();
        if m.start() < first_class && &c[1] == "Request" {
            prefix = path;
            continue;
        }
        let handler = JAVA_METHOD
            .captures(&t[m.end()..])
            .map(|h| h[3].to_string());
        let method = if &c[1] == "Request" {
            "ANY".to_string()
        } else {
            c[1].to_ascii_uppercase()
        };
        out.routes.push(Route {
            method,
            path: normalize_slashes(&format!("{prefix}/{path}")),
            handler,
            framework: "spring".into(),
            line: ctx.line(m.start()),
            receiver: None,
        });
    }
    // ASP.NET routes.
    let class_name = class_ranges
        .first()
        .map(|r| r.2.trim_end_matches("Controller").to_ascii_lowercase())
        .unwrap_or_default();
    let aspnet_prefix = ASPNET_PREFIX
        .captures(t)
        .filter(|c| c.get(0).unwrap().start() < first_class)
        .map(|c| c[1].replace("[controller]", &class_name))
        .unwrap_or_default();
    for c in ASPNET_ROUTE.captures_iter(t) {
        let m = c.get(0).unwrap();
        let handler = JAVA_METHOD
            .captures(&t[m.end()..])
            .map(|h| h[3].to_string());
        let sub = opt_str(c.get(2)).unwrap_or_default();
        out.routes.push(Route {
            method: c[1].to_ascii_uppercase(),
            path: normalize_slashes(&format!("{aspnet_prefix}/{sub}")),
            handler,
            framework: "aspnet".into(),
            line: ctx.line(m.start()),
            receiver: None,
        });
    }

    let c = &mut out.concepts;
    c.insert("reflection".into(), count(&JVM_REFLECT, t));
    c.insert("exceptions".into(), count(&JVM_TRY, t));
    c.insert("null_values".into(), count(&JVM_NULL, t));
    c.insert("async".into(), count(&JVM_ASYNC, t));
    c.insert("threads".into(), count(&JVM_THREADS, t));
    c.insert("decorators".into(), count(&JVM_ANNOT, t));
    c.insert("iterators".into(), count(&JVM_STREAMS, t));
    c.insert("any_type".into(), count(&JVM_OBJECT, t));
    out
}

// ---------------------------------------------------------------------------
// Kotlin
// ---------------------------------------------------------------------------

re!(KT_IMPORT, r"(?m)^[ \t]*import[ \t]+([\w\.\*]+)");
re!(
    KT_CLASS,
    r"(?m)^[ \t]*((?:(?:public|private|internal|open|abstract|sealed|data|enum|inner|value)[ \t]+)*)(class|interface|object)[ \t]+(\w+)(?:<[^>\n]*>)?[ \t]*(?:\(PARAMS\))?([^{\n]*)"
);
re!(
    KT_FUN,
    r"(?m)^[ \t]*((?:(?:public|private|internal|protected|override|open|suspend|inline|operator|infix|tailrec)[ \t]+)*)fun[ \t]+(?:<[^>]+>[ \t]*)?(?:(\w+)\.)?(\w+)[ \t]*\(PARAMS\)[ \t]*(?::[ \t]*([\w<>\?, \.\*]+?))?[ \t]*(?:\{|=)"
);
re!(KT_NULL, r"\?\.|!!|\bnull\b");
re!(KT_ASYNC, r"\bsuspend\b|\blaunch\b|\basync\s*\{|Flow<");

fn kotlin(ctx: &Ctx) -> Extracted {
    let t = ctx.text;
    let mut out = Extracted::default();
    for c in KT_IMPORT.captures_iter(t) {
        out.imports.push(c[1].to_string());
    }
    let mut class_ranges: Vec<(usize, usize, String)> = vec![];
    for c in KT_CLASS.captures_iter(t) {
        let m = c.get(0).unwrap();
        let name = c[3].to_string();
        let kind = match (&c[2], c[1].contains("enum"), c[1].contains("data")) {
            ("interface", _, _) => SymbolKind::Interface,
            (_, true, _) => SymbolKind::Enum,
            (_, _, true) => SymbolKind::Struct,
            _ => SymbolKind::Class,
        };
        let has_body = c
            .get(5)
            .is_some_and(|r| r.as_str().trim_end().ends_with('{'))
            || t[m.end()..].starts_with('{');
        let (start, end) = if has_body {
            ctx.block_range(m.start())
        } else {
            (m.start(), m.end())
        };
        let mut s = sym(
            kind,
            &name,
            ctx.line(m.start()),
            ctx.line(end.saturating_sub(1)),
            m.as_str(),
        );
        if let Some(p) = c.get(4) {
            for part in split_top_level(p.as_str()) {
                if part.starts_with("val ")
                    || part.contains(" val ")
                    || part.starts_with("var ")
                    || part.contains(" var ")
                {
                    let part = part.replace("val ", "").replace("var ", "");
                    for p in parse_params(Language::Kotlin, &part) {
                        let optional = p.ty.as_deref().is_some_and(|t| t.ends_with('?'));
                        s.fields.push(Field {
                            name: p.name,
                            ty: p.ty,
                            optional,
                        });
                    }
                }
            }
        }
        if let Some(rest) = c.get(5) {
            let rest = rest
                .as_str()
                .trim()
                .trim_start_matches(':')
                .trim_end_matches('{');
            s.bases = split_top_level(rest)
                .into_iter()
                .map(|b| b.split('(').next().unwrap_or("").trim().to_string())
                .filter(|b| !b.is_empty())
                .collect();
        }
        class_ranges.push((start, end, name));
        out.symbols.push(s);
    }
    for c in KT_FUN.captures_iter(t) {
        let m = c.get(0).unwrap();
        let parent = c
            .get(2)
            .map(|x| x.as_str().to_string())
            .or_else(|| enclosing(&class_ranges, m.start()).map(|r| r.2.clone()));
        let kind = if parent.is_some() {
            SymbolKind::Method
        } else {
            SymbolKind::Function
        };
        let end = if m.as_str().ends_with('{') {
            ctx.end_line(m.start())
        } else {
            ctx.line(m.end())
        };
        let mut s = sym(kind, &c[3], ctx.line(m.start()), end, m.as_str());
        s.parent = parent;
        s.is_async = c[1].contains("suspend");
        s.params = parse_params(Language::Kotlin, &c[4]);
        s.return_type = opt_str(c.get(5));
        out.symbols.push(s);
    }
    let c = &mut out.concepts;
    c.insert("null_values".into(), count(&KT_NULL, t));
    c.insert("async".into(), count(&KT_ASYNC, t));
    c.insert("exceptions".into(), count(&JVM_TRY, t));
    c.insert("reflection".into(), count(&JVM_REFLECT, t));
    c.insert("decorators".into(), count(&JVM_ANNOT, t));
    out
}

// ---------------------------------------------------------------------------
// C / C++
// ---------------------------------------------------------------------------

re!(
    C_INCLUDE,
    r#"(?m)^[ \t]*#[ \t]*include[ \t]*[<"]([^>"]+)[>"]"#
);
re!(
    C_FUNC,
    r"(?m)^([A-Za-z_][\w\*&:<>, \t]*?)[ \t\*&]+(\**~?[A-Za-z_]\w*(?:::~?\w+)*)[ \t]*\(PARAMS\)[ \t]*(?:const)?[ \t]*(?:noexcept)?[ \t]*(?:override)?[ \t]*\{"
);
re!(
    C_STRUCT,
    r"(?m)^[ \t]*(?:typedef[ \t]+)?struct[ \t]+(\w+)?[ \t]*\{"
);
re!(
    CPP_CLASS,
    r"(?m)^[ \t]*class[ \t]+(\w+)(?:[ \t]+final)?(?:[ \t]*:[ \t]*([^{\n]+))?[ \t]*\{"
);
re!(
    C_FIELD,
    r"^[ \t]*([A-Za-z_][\w \t\*:<>,]*?)[ \t\*&]+(\w+)[ \t]*(\[[^\]]*\])?[ \t]*;"
);
re!(C_TYPEDEF_NAME, r"^\s*(\w+)\s*;");
re!(
    C_MEM,
    r"\b(?:malloc|calloc|realloc|free)\s*\(|\bnew\s+\w|\bdelete\b"
);
re!(C_PTR, r"->|\*\s*\w+\s*=|&\w+");
re!(C_MACRO, r"(?m)^[ \t]*#[ \t]*define\b");
re!(
    C_THREADS,
    r"\bpthread_\w+|std::thread|std::mutex|std::async"
);
re!(C_TRY, r"\bthrow\b|\btry\s*\{");
re!(C_NULL, r"\bNULL\b|\bnullptr\b");
re!(C_TEMPLATE, r"\btemplate\s*<");
re!(C_GOTO, r"\bgoto\b");

fn c_like(ctx: &Ctx) -> Extracted {
    let t = ctx.text;
    let mut out = Extracted::default();
    for c in C_INCLUDE.captures_iter(t) {
        out.imports.push(c[1].to_string());
    }
    for c in C_FUNC.captures_iter(t) {
        let m = c.get(0).unwrap();
        let raw_name = c[2].trim_start_matches('*').to_string();
        let ret = c[1]
            .split_whitespace()
            .filter(|w| {
                ![
                    "static",
                    "inline",
                    "extern",
                    "virtual",
                    "constexpr",
                    "explicit",
                ]
                .contains(w)
            })
            .collect::<Vec<_>>()
            .join(" ");
        let first = ret.split_whitespace().next().unwrap_or("");
        if KEYWORDS.contains(&raw_name.as_str())
            || KEYWORDS.contains(&first)
            || ret.is_empty()
            || first == "return"
            || first == "else"
        {
            continue;
        }
        let (parent, name) = match raw_name.rsplit_once("::") {
            Some((p, n)) => (
                Some(p.rsplit("::").next().unwrap_or(p).to_string()),
                n.to_string(),
            ),
            None => (None, raw_name),
        };
        let kind = if parent.is_some() {
            SymbolKind::Method
        } else {
            SymbolKind::Function
        };
        let mut s = sym(
            kind,
            &name,
            ctx.line(m.start()),
            ctx.end_line(m.start()),
            m.as_str(),
        );
        s.parent = parent;
        s.params = parse_params(ctx.lang, &c[3]);
        let ptr = if c[2].starts_with('*') { "*" } else { "" };
        s.return_type = Some(format!("{ret}{ptr}")).filter(|r| r != "void");
        out.symbols.push(s);
    }
    for (re, kind) in [
        (&*C_STRUCT, SymbolKind::Struct),
        (&*CPP_CLASS, SymbolKind::Class),
    ] {
        for c in re.captures_iter(t) {
            let m = c.get(0).unwrap();
            let (start, end) = ctx.block_range(m.start());
            let name = opt_str(c.get(1))
                .or_else(|| C_TYPEDEF_NAME.captures(&t[end..]).map(|n| n[1].to_string()));
            let Some(name) = name else { continue };
            let mut s = sym(
                kind,
                &name,
                ctx.line(m.start()),
                ctx.line(end.saturating_sub(1)),
                m.as_str(),
            );
            if kind == SymbolKind::Class {
                if let Some(b) = c.get(2) {
                    s.bases = split_top_level(b.as_str())
                        .into_iter()
                        .map(|b| {
                            b.replace("public", "")
                                .replace("private", "")
                                .replace("protected", "")
                                .replace("virtual", "")
                                .trim()
                                .to_string()
                        })
                        .collect();
                }
            }
            let inner = t[start..end].split_once('{').map(|x| x.1).unwrap_or("");
            let mut depth = 0i32;
            for l in inner.lines() {
                let before = depth;
                depth += l.matches('{').count() as i32 - l.matches('}').count() as i32;
                if before != 0 || l.contains('(') {
                    continue;
                }
                if let Some(f) = C_FIELD.captures(l) {
                    let mut ty = f[1].trim().to_string();
                    if l.contains('*') {
                        ty.push('*');
                    }
                    if let Some(arr) = f.get(3) {
                        ty.push_str(arr.as_str());
                    }
                    if ["return", "public", "private", "protected"].contains(&ty.as_str()) {
                        continue;
                    }
                    s.fields.push(Field {
                        name: f[2].to_string(),
                        ty: Some(ty),
                        optional: false,
                    });
                }
            }
            out.symbols.push(s);
        }
    }
    let c = &mut out.concepts;
    c.insert("manual_memory".into(), count(&C_MEM, t));
    c.insert("unsafe_pointers".into(), count(&C_PTR, t));
    c.insert("macros".into(), count(&C_MACRO, t));
    c.insert("threads".into(), count(&C_THREADS, t));
    c.insert("exceptions".into(), count(&C_TRY, t));
    c.insert("null_values".into(), count(&C_NULL, t));
    c.insert("generics".into(), count(&C_TEMPLATE, t));
    c.insert("goto".into(), count(&C_GOTO, t));
    out
}

// ---------------------------------------------------------------------------
// Ruby
// ---------------------------------------------------------------------------

re!(
    RB_REQUIRE,
    r#"(?m)^[ \t]*require(?:_relative)?[ \t]*\(?[ \t]*['"]([^'"]+)['"]"#
);
re!(
    RB_CLASS,
    r"(?m)^([ \t]*)(class|module)[ \t]+([\w:]+)(?:[ \t]*<[ \t]*([\w:]+))?"
);
re!(
    RB_DEF,
    r"(?m)^([ \t]*)def[ \t]+(self\.)?(\w+[?!=]?)(?:[ \t]*\(([^)]*)\)|[ \t]+([\w, \*&:=]+))?"
);
re!(
    RB_ATTR,
    r"attr_(?:accessor|reader|writer)[ \t]+([:\w, \t]+)"
);
re!(
    RB_SINATRA,
    r#"(?m)^[ \t]*(get|post|put|patch|delete)[ \t]+['"]([^'"]+)['"](?:[ \t]*,[ \t]*to:[ \t]*['"]([^'"]+)['"])?"#
);
re!(RB_RESOURCES, r"(?m)^[ \t]*resources?[ \t]+:(\w+)");
re!(
    RB_DYN,
    r"\b(?:eval|instance_eval|class_eval|define_method|method_missing|send|public_send|instance_variable_get)\b"
);
re!(RB_RESCUE, r"\brescue\b|\braise\b");
re!(RB_NIL, r"\bnil\b|&\.");
re!(RB_BLOCK, r"\bdo\s*\||\{\s*\|");
re!(
    RB_MIXIN,
    r"(?m)^[ \t]*(?:include|extend|prepend)[ \t]+[A-Z]"
);

fn ruby(ctx: &Ctx) -> Extracted {
    let t = ctx.text;
    let mut out = Extracted::default();
    for c in RB_REQUIRE.captures_iter(t) {
        out.imports.push(c[1].to_string());
    }
    enum Item<'t> {
        Class(regex::Captures<'t>),
        Def(regex::Captures<'t>),
    }
    let mut items: Vec<(usize, Item)> = RB_CLASS
        .captures_iter(t)
        .map(|c| (c.get(0).unwrap().start(), Item::Class(c)))
        .chain(
            RB_DEF
                .captures_iter(t)
                .map(|c| (c.get(0).unwrap().start(), Item::Def(c))),
        )
        .collect();
    items.sort_by_key(|(o, _)| *o);
    let mut stack: Vec<(usize, String, bool)> = vec![];
    for (offset, item) in items {
        let indent = match &item {
            Item::Class(c) | Item::Def(c) => indent_of(&c[1]),
        };
        while stack.last().is_some_and(|(i, _, _)| *i >= indent) {
            stack.pop();
        }
        let parent = stack
            .iter()
            .rev()
            .find(|(_, _, is_class)| *is_class)
            .map(|(_, n, _)| n.clone());
        let line = ctx.line(offset);
        let end = ctx.end_line(offset);
        match item {
            Item::Class(c) => {
                let name = c[3].rsplit("::").next().unwrap_or(&c[3]).to_string();
                let mut s = sym(
                    SymbolKind::Class,
                    &name,
                    line,
                    end,
                    c.get(0).unwrap().as_str(),
                );
                if let Some(b) = c.get(4) {
                    s.bases.push(b.as_str().to_string());
                }
                let body = crate::text::slice_lines(t, line, end);
                for a in RB_ATTR.captures_iter(&body) {
                    for n in split_names(&a[1]) {
                        s.fields.push(Field {
                            name: n,
                            ty: None,
                            optional: false,
                        });
                    }
                }
                stack.push((indent, name, true));
                out.symbols.push(s);
            }
            Item::Def(c) => {
                let name = c[3].to_string();
                let kind = if parent.is_some() {
                    SymbolKind::Method
                } else {
                    SymbolKind::Function
                };
                let mut s = sym(kind, &name, line, end, c.get(0).unwrap().as_str());
                s.parent = parent;
                let raw = c.get(4).or(c.get(5)).map(|m| m.as_str()).unwrap_or("");
                s.params = parse_params(Language::Ruby, raw);
                if c.get(2).is_some() {
                    s.signature = format!("static {}", s.signature);
                }
                stack.push((indent, name, false));
                out.symbols.push(s);
            }
        }
    }
    for c in RB_SINATRA.captures_iter(t) {
        out.routes.push(Route {
            method: c[1].to_ascii_uppercase(),
            path: c[2].to_string(),
            handler: opt_str(c.get(3)).map(|h| h.replace('#', "_")),
            framework: "rails/sinatra".into(),
            line: ctx.line(c.get(0).unwrap().start()),
            receiver: None,
        });
    }
    for c in RB_RESOURCES.captures_iter(t) {
        out.routes.push(Route {
            method: "REST".into(),
            path: format!("/{}", &c[1]),
            handler: Some(c[1].to_string()),
            framework: "rails".into(),
            line: ctx.line(c.get(0).unwrap().start()),
            receiver: None,
        });
    }
    let c = &mut out.concepts;
    c.insert("dynamic_eval".into(), count(&RB_DYN, t));
    c.insert("exceptions".into(), count(&RB_RESCUE, t));
    c.insert("null_values".into(), count(&RB_NIL, t));
    c.insert("closures".into(), count(&RB_BLOCK, t));
    c.insert("mixins".into(), count(&RB_MIXIN, t));
    out
}

// ---------------------------------------------------------------------------
// PHP
// ---------------------------------------------------------------------------

re!(PHP_USE, r"(?m)^[ \t]*use[ \t]+([\w\\]+)");
re!(
    PHP_REQUIRE,
    r#"\b(?:require|include)(?:_once)?[ \t]*\(?[ \t]*['"]([^'"]+)['"]"#
);
re!(
    PHP_CLASS,
    r"(?m)^[ \t]*(?:(?:abstract|final|readonly)[ \t]+)*(class|interface|trait|enum)[ \t]+(\w+)([^{\n]*)"
);
re!(
    PHP_FUNC,
    r"(?m)^[ \t]*((?:(?:public|private|protected|static|abstract|final)[ \t]+)*)function[ \t]+&?(\w+)[ \t]*\(PARAMS\)[ \t]*(?::[ \t]*(\??[\w\\|]+))?"
);
re!(
    PHP_PROP,
    r"(?m)^[ \t]*(?:public|private|protected)(?:[ \t]+(?:readonly|static))?[ \t]+(\??[\w\\]+)?[ \t]*\$(\w+)"
);
re!(
    PHP_ROUTE,
    r#"Route::(get|post|put|patch|delete|any)\(\s*['"]([^'"]+)['"]\s*,PARAMS\)"#
);
re!(PHP_DYN, r"\beval\s*\(|\$\$\w|__call|__get|call_user_func");
re!(PHP_TRY, r"\bcatch\s*\(|\bthrow\b");
re!(PHP_NULL, r"\bnull\b");

fn php(ctx: &Ctx) -> Extracted {
    let t = ctx.text;
    let mut out = Extracted::default();
    for re in [&*PHP_USE, &*PHP_REQUIRE] {
        for c in re.captures_iter(t) {
            out.imports.push(c[1].to_string());
        }
    }
    let mut class_ranges: Vec<(usize, usize, String)> = vec![];
    for c in PHP_CLASS.captures_iter(t) {
        let m = c.get(0).unwrap();
        let (start, end) = ctx.block_range(m.start());
        let name = c[2].to_string();
        let kind = match &c[1] {
            "interface" | "trait" => SymbolKind::Interface,
            "enum" => SymbolKind::Enum,
            _ => SymbolKind::Class,
        };
        let mut s = sym(
            kind,
            &name,
            ctx.line(m.start()),
            ctx.line(end.saturating_sub(1)),
            m.as_str(),
        );
        let rest = c[3].replace("extends", ",").replace("implements", ",");
        s.bases = split_top_level(&rest)
            .into_iter()
            .filter(|b| !b.is_empty() && b != "{")
            .collect();
        class_ranges.push((start, end, name));
        out.symbols.push(s);
    }
    for c in PHP_FUNC.captures_iter(t) {
        let m = c.get(0).unwrap();
        let parent = enclosing(&class_ranges, m.start()).map(|r| r.2.clone());
        let raw = &c[2];
        let name = if raw == "__construct" {
            "constructor"
        } else {
            raw
        };
        let kind = if parent.is_some() {
            SymbolKind::Method
        } else {
            SymbolKind::Function
        };
        let mut s = sym(
            kind,
            name,
            ctx.line(m.start()),
            ctx.end_line(m.start()),
            m.as_str(),
        );
        s.parent = parent;
        s.params = parse_params(Language::Php, &c[3]);
        s.return_type = opt_str(c.get(4)).filter(|r| r != "void");
        if c[1].contains("static") {
            s.signature = format!("static {}", s.signature);
        }
        out.symbols.push(s);
    }
    for c in PHP_PROP.captures_iter(t) {
        let off = c.get(0).unwrap().start();
        if let Some((_, _, class)) = enclosing(&class_ranges, off) {
            if let Some(s) = out
                .symbols
                .iter_mut()
                .find(|s| &s.name == class && s.kind != SymbolKind::Method)
            {
                let ty = opt_str(c.get(1));
                let optional = ty.as_deref().is_some_and(|t| t.starts_with('?'));
                s.fields.push(Field {
                    name: c[2].to_string(),
                    ty,
                    optional,
                });
            }
        }
    }
    for c in PHP_ROUTE.captures_iter(t) {
        let args = &c[3];
        let handler = args
            .rsplit(['\'', '"'])
            .find(|s| {
                s.chars()
                    .all(|ch| ch.is_alphanumeric() || ch == '_' || ch == '@')
                    && !s.is_empty()
            })
            .map(|h| h.rsplit('@').next().unwrap_or(h).to_string());
        out.routes.push(Route {
            method: c[1].to_ascii_uppercase(),
            path: normalize_slashes(&c[2]),
            handler,
            framework: "laravel".into(),
            line: ctx.line(c.get(0).unwrap().start()),
            receiver: None,
        });
    }
    let c = &mut out.concepts;
    c.insert("dynamic_eval".into(), count(&PHP_DYN, t));
    c.insert("exceptions".into(), count(&PHP_TRY, t));
    c.insert("null_values".into(), count(&PHP_NULL, t));
    out
}

// ---------------------------------------------------------------------------
// Parameters
// ---------------------------------------------------------------------------

/// Split `name: type = default` style text at the first top-level `=` that is
/// not part of `=>`, `==`, `>=` or `<=`.
fn strip_default(p: &str) -> &str {
    let bytes = p.as_bytes();
    let mut depth = 0i32;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'(' | b'[' | b'{' | b'<' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b'>' if i == 0 || bytes[i - 1] != b'=' => depth -= 1,
            b'=' if depth <= 0 => {
                let next = bytes.get(i + 1).copied();
                let prev = if i > 0 { bytes[i - 1] } else { b' ' };
                if next != Some(b'>')
                    && next != Some(b'=')
                    && !matches!(prev, b'!' | b'<' | b'>' | b'=')
                {
                    return p[..i].trim();
                }
            }
            _ => {}
        }
    }
    p.trim()
}

fn ident_prefix(s: &str) -> String {
    s.chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect()
}

pub fn parse_params(lang: Language, raw: &str) -> Vec<Param> {
    let raw = squash(raw);
    let mut out: Vec<Param> = vec![];
    for (i, part) in split_top_level(&raw).into_iter().enumerate() {
        let part = part.trim();
        let fallback = || format!("arg{i}");
        let p = match lang {
            Language::Python => {
                let base = strip_default(part);
                if matches!(base, "self" | "cls" | "*" | "/") {
                    continue;
                }
                let (name, ty) = match base.split_once(':') {
                    Some((n, t)) => (n.trim(), Some(t.trim().to_string())),
                    None => (base, None),
                };
                if let Some(n) = name.strip_prefix("**") {
                    Param {
                        name: n.to_string(),
                        ty: Some("dict".into()),
                    }
                } else if let Some(n) = name.strip_prefix('*') {
                    Param {
                        name: n.to_string(),
                        ty: Some("list".into()),
                    }
                } else {
                    let ty =
                        ty.or_else(|| part.contains('=').then(|| infer_literal(part)).flatten());
                    Param {
                        name: name.to_string(),
                        ty,
                    }
                }
            }
            Language::JavaScript | Language::TypeScript => {
                let mut base = strip_default(part).to_string();
                for m in [
                    "public ",
                    "private ",
                    "protected ",
                    "readonly ",
                    "override ",
                ] {
                    base = base.replace(m, "");
                }
                let base = base.trim();
                if base.starts_with('{') || base.starts_with('[') {
                    let ty = base
                        .rsplit_once("}:")
                        .or_else(|| base.rsplit_once("]:"))
                        .map(|(_, t)| t.trim().to_string());
                    Param {
                        name: if base.starts_with('{') {
                            "params".into()
                        } else {
                            fallback()
                        },
                        ty,
                    }
                } else {
                    let (name, ty) = match base.split_once(':') {
                        Some((n, t)) => (n.trim(), Some(t.trim().to_string())),
                        None => (base, None),
                    };
                    let optional = name.ends_with('?') || part.contains('=');
                    let name = name.trim_end_matches('?');
                    if let Some(n) = name.strip_prefix("...") {
                        Param {
                            name: n.to_string(),
                            ty: Some(ty.unwrap_or_else(|| "any[]".into())),
                        }
                    } else {
                        let ty = ty
                            .or_else(|| part.contains('=').then(|| infer_literal(part)).flatten());
                        let ty = match (optional, ty) {
                            (true, Some(t)) if !t.contains("undefined") => {
                                Some(format!("{t} | undefined"))
                            }
                            (_, t) => t,
                        };
                        Param {
                            name: name.to_string(),
                            ty,
                        }
                    }
                }
            }
            Language::Go => {
                let mut words = part.split_whitespace();
                let first = words.next().unwrap_or("");
                let rest: Vec<&str> = words.collect();
                if rest.is_empty() {
                    // Either a bare type (unnamed param) or a grouped name.
                    Param {
                        name: first.to_string(),
                        ty: None,
                    }
                } else {
                    let ty = rest.join(" ");
                    let ty = ty
                        .strip_prefix("...")
                        .map(|t| format!("[]{t}"))
                        .unwrap_or(ty);
                    Param {
                        name: first.to_string(),
                        ty: Some(ty),
                    }
                }
            }
            Language::Kotlin => {
                let base = strip_default(part);
                let base = base.trim_start_matches("vararg ").trim();
                match base.split_once(':') {
                    Some((n, t)) => Param {
                        name: n.trim().to_string(),
                        ty: Some(t.trim().to_string()),
                    },
                    None => Param {
                        name: base.to_string(),
                        ty: None,
                    },
                }
            }
            Language::Java | Language::CSharp | Language::C | Language::Cpp => {
                let base = strip_default(part);
                let base: String = base
                    .split_whitespace()
                    .filter(|w| {
                        !w.starts_with('@')
                            && ![
                                "final", "this", "ref", "out", "in", "params", "const", "struct",
                                "register",
                            ]
                            .contains(w)
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                if base == "void" || base.is_empty() || base == "..." {
                    continue;
                }
                let base = base.replace(" *", "* ").replace(" &", "& ");
                let (ty, name) = match base.rsplit_once(' ') {
                    Some((t, n)) => (t.trim().to_string(), n.trim().to_string()),
                    None => (base.clone(), fallback()),
                };
                let (name, ty) = if let Some((n, arr)) = name.split_once('[') {
                    (n.to_string(), format!("{ty}[{arr}"))
                } else {
                    (name, ty)
                };
                let name = name.trim_start_matches(&['*', '&'][..]).to_string();
                Param { name, ty: Some(ty) }
            }
            Language::Ruby => {
                let base = strip_default(part);
                let name = base
                    .trim_start_matches(&['*', '&'][..])
                    .trim_end_matches(':');
                let ty = if base.starts_with("**") {
                    Some("dict".into())
                } else if base.starts_with('*') {
                    Some("list".into())
                } else {
                    None
                };
                Param {
                    name: name.split(':').next().unwrap_or(name).trim().to_string(),
                    ty,
                }
            }
            Language::Php => {
                let base = strip_default(part);
                let mut words: Vec<&str> = base
                    .split_whitespace()
                    .filter(|w| !["public", "private", "protected", "readonly"].contains(w))
                    .collect();
                let name = words
                    .pop()
                    .unwrap_or("")
                    .trim_start_matches("...")
                    .trim_start_matches('&')
                    .trim_start_matches('$')
                    .to_string();
                let ty = words.last().map(|t| t.to_string());
                Param { name, ty }
            }
            _ => Param {
                name: fallback(),
                ty: None,
            },
        };
        let name = ident_prefix(&p.name);
        out.push(Param {
            name: if name.is_empty() { fallback() } else { name },
            ty: p.ty,
        });
    }
    // Go: back-fill grouped names `a, b int`.
    if lang == Language::Go {
        let mut last: Option<String> = None;
        for p in out.iter_mut().rev() {
            match &p.ty {
                Some(t) => last = Some(t.clone()),
                None => p.ty = last.clone().or_else(|| Some(p.name.clone())),
            }
        }
    }
    out
}

fn infer_literal(part: &str) -> Option<String> {
    let (_, v) = part.split_once('=')?;
    let v = v.trim();
    Some(
        if v == "True" || v == "False" || v == "true" || v == "false" {
            "bool"
        } else if v.parse::<i64>().is_ok() {
            "int"
        } else if v.parse::<f64>().is_ok() {
            "float"
        } else if v.starts_with('"') || v.starts_with('\'') {
            "str"
        } else {
            return None;
        }
        .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn python_shapes() {
        let src = r#"
import os
from .models import Loan, Customer
from flask import Flask

@dataclass
class Loan:
    principal: float
    rate: float = 0.1
    tenure: Optional[int] = None

    def emi(self, months: int = 12) -> float:
        return 1.0

@app.route("/loans/<int:loan_id>", methods=["GET", "DELETE"])
def get_loan(loan_id: int):
    pass

async def fetch(url: str, *args, **kwargs) -> dict:
    pass
"#;
        let e = extract(Language::Python, src);
        assert!(
            e.imports.contains(&".models".to_string())
                || e.imports.contains(&".Loan".to_string())
                || e.imports.iter().any(|i| i.contains("models"))
        );
        let loan = e.symbols.iter().find(|s| s.name == "Loan").unwrap();
        assert_eq!(loan.fields.len(), 3);
        assert!(loan.fields[2].optional);
        let emi = e.symbols.iter().find(|s| s.name == "emi").unwrap();
        assert_eq!(emi.parent.as_deref(), Some("Loan"));
        assert_eq!(emi.params.len(), 1);
        assert_eq!(e.routes.len(), 2);
        assert_eq!(e.routes[0].handler.as_deref(), Some("get_loan"));
        let fetch = e.symbols.iter().find(|s| s.name == "fetch").unwrap();
        assert!(fetch.is_async);
        assert_eq!(fetch.params.len(), 3);
    }

    #[test]
    fn typescript_shapes() {
        let src = r#"
import express from 'express';
export interface User {
  id: string;
  email?: string;
  roles: string[];
}
export class UserService extends Base {
  private repo: Repo;
  constructor(repo: Repo) { this.repo = repo; }
  async find(id: string): Promise<User | null> {
    if (id) { return null; }
  }
}
export const add = (a: number, b: number): number => a + b;
app.get('/users/:id', getUser);
router.post('/users', async (req, res) => { res.send('ok'); });
axios.get('/not-a-route');
"#;
        let e = extract(Language::TypeScript, src);
        assert!(e.imports.contains(&"express".to_string()));
        let user = e.symbols.iter().find(|s| s.name == "User").unwrap();
        assert_eq!(user.fields.len(), 3);
        assert!(user.fields[1].optional);
        let find = e.symbols.iter().find(|s| s.name == "find").unwrap();
        assert_eq!(find.parent.as_deref(), Some("UserService"));
        assert!(find.is_async);
        assert!(e
            .symbols
            .iter()
            .any(|s| s.name == "add" && s.params.len() == 2));
        assert_eq!(e.routes.len(), 2);
        assert_eq!(e.routes[0].handler.as_deref(), Some("getUser"));
    }

    #[test]
    fn go_shapes() {
        let src = r#"
package main
import (
    "fmt"
    "net/http"
)
type Account struct {
    ID      int64  `json:"id"`
    Name, Owner string
    Balance *float64
}
func (a *Account) Deposit(amount float64) error {
    return nil
}
func add(a, b int) (int, error) { return a + b, nil }
func main() {
    http.HandleFunc("GET /accounts", listAccounts)
}
"#;
        let e = extract(Language::Go, src);
        let acct = e.symbols.iter().find(|s| s.name == "Account").unwrap();
        assert_eq!(acct.fields.len(), 4);
        let dep = e.symbols.iter().find(|s| s.name == "Deposit").unwrap();
        assert_eq!(dep.parent.as_deref(), Some("Account"));
        let add = e.symbols.iter().find(|s| s.name == "add").unwrap();
        assert_eq!(add.params[0].ty.as_deref(), Some("int"));
        assert_eq!(e.routes[0].method, "GET");
        assert_eq!(e.routes[0].path, "/accounts");
    }

    #[test]
    fn java_shapes() {
        let src = r#"
package com.bank;
import java.math.BigDecimal;
@RestController
@RequestMapping("/api/loans")
public class LoanController {
    private final LoanService service;
    private BigDecimal limit;
    public LoanController(LoanService service) { this.service = service; }
    @GetMapping("/{id}")
    public Loan get(@PathVariable Long id) { return service.find(id); }
    @PostMapping
    public ResponseEntity<Loan> create(@RequestBody Loan loan) throws Exception { return null; }
}
"#;
        let e = extract(Language::Java, src);
        let c = e
            .symbols
            .iter()
            .find(|s| s.name == "LoanController")
            .unwrap();
        assert_eq!(c.fields.len(), 2);
        assert!(e.symbols.iter().any(|s| s.name == "constructor"));
        let get = e.symbols.iter().find(|s| s.name == "get").unwrap();
        assert_eq!(get.params[0].name, "id");
        assert_eq!(get.params[0].ty.as_deref(), Some("Long"));
        assert_eq!(e.routes.len(), 2);
        assert_eq!(e.routes[0].path, "/api/loans/{id}");
        assert_eq!(e.routes[1].handler.as_deref(), Some("create"));
    }
}
