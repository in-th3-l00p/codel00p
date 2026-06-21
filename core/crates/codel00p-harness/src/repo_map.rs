//! `repo_map`: a ranked, compact map of the code symbols in the workspace.
//!
//! This is the navigation aid mature coding agents (Aider's repo map, Claude
//! Code's codebase understanding) lean on: instead of reading whole files, the
//! model gets the most *important* definitions across the repo — functions,
//! types, classes — ranked so the symbols other code depends on most surface
//! first.
//!
//! Symbol extraction is **AST-based** (tree-sitter) for the highest-value
//! languages — Rust, Python, JavaScript, TypeScript, Go — which gives accurate
//! symbol kinds and signatures and avoids the false positives/misses of line
//! regexes (e.g. a `fn` name inside a string or comment is never extracted, and
//! multi-line signatures are captured in full). Languages without a tree-sitter
//! grammar here (Java/Kotlin, Ruby, C/C++) fall back to the historical per-line
//! regex heuristic, so the tool never regresses.
//!
//! Importance is a cheap proxy for Aider's PageRank: a symbol's score is how
//! often its name is referenced across the whole repo, and a file's score is the
//! sum of its symbols' scores. It is deterministic and good enough to point the
//! model at the right files fast; for exact navigation it pairs with `grep` /
//! `read_file`.

use std::{collections::HashMap, sync::OnceLock};

use async_trait::async_trait;
use codel00p_protocol::PermissionScope;
use regex::Regex;
use serde_json::{Value, json};
use tree_sitter::{Node, Parser, Query, QueryCursor, StreamingIterator};

use crate::{
    errors::HarnessError,
    tool_result::ToolResult,
    tools::{Tool, Verbosity, optional_string, parse_verbosity, verbosity_schema},
    walk::GlobMatcher,
    workspace::Workspace,
};

/// Default and ceiling for the number of files included in the map.
const DEFAULT_MAX_FILES: usize = 50;
const MAX_MAX_FILES: usize = 500;
/// Default and ceiling for the number of symbols listed per file.
const DEFAULT_MAX_SYMBOLS_PER_FILE: usize = 20;
const MAX_MAX_SYMBOLS_PER_FILE: usize = 100;
/// Source files larger than this are skipped (assumed generated/minified).
const MAX_SOURCE_FILE_BYTES: u64 = 2 * 1_024 * 1_024;
/// A symbol signature line is trimmed to this many characters.
const MAX_SIGNATURE_CHARS: usize = 200;

pub struct RepoMapTool;

#[async_trait]
impl Tool for RepoMapTool {
    fn name(&self) -> &str {
        "repo_map"
    }

    fn description(&self) -> &str {
        "Produce a ranked map of the code symbols (functions, types, classes, …) \
         across the workspace, so you can orient in an unfamiliar codebase without \
         reading whole files. Symbols and files are ranked by how often they are \
         referenced elsewhere, most-depended-on first. Restrict scope with `path` \
         and/or a `glob` file filter, and bound output with `max_files` and \
         `max_symbols_per_file`. Supports Rust, Python, JavaScript/TypeScript, Go, \
         Java, Ruby, and C/C++. Pair with `grep`/`read_file` for exact navigation. \
         `verbosity` controls per-symbol detail: \"detailed\" (default) lists each \
         symbol with its `kind`, `line`, full `signature`, and `references`; \
         \"concise\" lists symbol names only (dropping signatures) for a cheap \
         name-level overview."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "glob": { "type": "string" },
                "include_ignored": { "type": "boolean" },
                "max_files": { "type": "integer", "minimum": 1, "maximum": MAX_MAX_FILES },
                "max_symbols_per_file": {
                    "type": "integer", "minimum": 1, "maximum": MAX_MAX_SYMBOLS_PER_FILE
                },
                "verbosity": verbosity_schema()
            }
        })
    }

    fn is_concurrency_safe(&self, _input: &Value) -> bool {
        true
    }

    fn permission_scope(&self, _input: &Value) -> PermissionScope {
        PermissionScope::ReadOnly
    }

    async fn execute(
        &self,
        workspace: &Workspace,
        input: Value,
    ) -> Result<ToolResult, HarnessError> {
        let path = optional_string(&input, "path").unwrap_or(".");
        let verbosity = parse_verbosity(self.name(), &input)?;
        let include_ignored = input
            .get("include_ignored")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let max_files = clamped(&input, "max_files", DEFAULT_MAX_FILES, MAX_MAX_FILES);
        let max_symbols_per_file = clamped(
            &input,
            "max_symbols_per_file",
            DEFAULT_MAX_SYMBOLS_PER_FILE,
            MAX_MAX_SYMBOLS_PER_FILE,
        );
        let glob = match optional_string(&input, "glob") {
            Some(glob) => Some(GlobMatcher::compile(self.name(), glob)?),
            None => None,
        };

        let mut relatives = Vec::new();
        workspace.walk(path, include_ignored, &mut |relative| {
            if glob.as_ref().is_none_or(|g| g.is_match(relative))
                && language_for(relative).is_some()
            {
                relatives.push(relative.to_string());
            }
        })?;
        relatives.sort();

        // First pass: read each source file once, extract its symbol definitions
        // and accumulate a global identifier-frequency table used for ranking.
        let mut file_symbols: Vec<(String, Vec<RawSymbol>)> = Vec::new();
        let mut reference_counts: HashMap<String, u32> = HashMap::new();
        let mut files_scanned = 0usize;
        let mut symbols_found = 0usize;

        for relative in &relatives {
            // Stat through the facade first and skip oversized files WITHOUT
            // reading them, so a pathological large file cannot be slurped into
            // memory. Unreadable/missing files are skipped, matching historical
            // behavior.
            let Ok(meta) = workspace.metadata(relative) else {
                continue;
            };
            if meta.size > MAX_SOURCE_FILE_BYTES {
                continue;
            }
            // Read through the workspace facade so a remote backend reads remote
            // files. Unreadable or non-UTF-8 files are skipped (assumed
            // generated/minified), matching the historical behavior.
            let Ok(bytes) = workspace.read_bytes(relative) else {
                continue;
            };
            let Ok(content) = String::from_utf8(bytes) else {
                continue;
            };
            files_scanned += 1;

            for ident in identifier_regex().find_iter(&content) {
                *reference_counts
                    .entry(ident.as_str().to_string())
                    .or_insert(0) += 1;
            }

            let dispatch = language_for(relative).expect("filtered to known languages");
            let symbols = extract_symbols(dispatch, &content);
            symbols_found += symbols.len();
            if !symbols.is_empty() {
                file_symbols.push((relative.clone(), symbols));
            }
        }

        // Second pass: score symbols by reference frequency, score each file by
        // the sum of its symbols' scores, and rank.
        let mut ranked_files: Vec<RankedFile> = file_symbols
            .into_iter()
            .map(|(path, mut symbols)| {
                for symbol in &mut symbols {
                    // Subtract the definition's own mention so a never-referenced
                    // symbol scores 0 rather than 1.
                    let total = reference_counts.get(&symbol.name).copied().unwrap_or(0);
                    symbol.score = total.saturating_sub(1);
                }
                symbols.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.line.cmp(&b.line)));
                let file_score: u64 = symbols.iter().map(|s| s.score as u64).sum();
                RankedFile {
                    path,
                    score: file_score,
                    symbols,
                }
            })
            .collect();
        ranked_files.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.path.cmp(&b.path)));

        let total_ranked = ranked_files.len();
        let truncated_files = total_ranked > max_files;

        let files: Vec<Value> = ranked_files
            .into_iter()
            .take(max_files)
            .map(|file| {
                let symbols: Vec<Value> = file
                    .symbols
                    .into_iter()
                    .take(max_symbols_per_file)
                    .map(|symbol| {
                        // Concise: symbol names only (no signatures), a cheap
                        // name-level overview. Detailed (default): the full,
                        // byte-identical historical shape.
                        if verbosity == Verbosity::Concise {
                            json!({ "name": symbol.name })
                        } else {
                            json!({
                                "name": symbol.name,
                                "kind": symbol.kind,
                                "line": symbol.line,
                                "signature": symbol.signature,
                                "references": symbol.score,
                            })
                        }
                    })
                    .collect();
                json!({
                    "path": file.path,
                    "score": file.score,
                    "symbols": symbols,
                })
            })
            .collect();

        Ok(ToolResult::json(json!({
            "root": path,
            "files_scanned": files_scanned,
            "symbols_found": symbols_found,
            "files": files,
            "truncated_files": truncated_files,
        })))
    }
}

/// A symbol definition before scoring.
struct RawSymbol {
    name: String,
    kind: &'static str,
    line: usize,
    signature: String,
    score: u32,
}

struct RankedFile {
    path: String,
    score: u64,
    symbols: Vec<RawSymbol>,
}

/// Read and clamp an optional positive-integer field.
fn clamped(input: &Value, key: &str, default: usize, max: usize) -> usize {
    input
        .get(key)
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(default)
        .clamp(1, max)
}

/// The identifier tokenizer used to build the global reference table.
fn identifier_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"[A-Za-z_][A-Za-z0-9_]*").expect("valid identifier regex"))
}

/// How a file's symbols are extracted: a tree-sitter AST (accurate) or the
/// historical per-line regex heuristic (fallback for languages without a
/// bundled grammar).
enum Dispatch {
    /// AST-based extraction via a compiled tree-sitter query.
    Ast(&'static TsLang),
    /// Per-line regex extraction.
    Regex(&'static Language),
}

/// Map a file path to its extraction strategy by extension, or `None` if
/// unsupported. Tree-sitter languages take precedence; the rest fall back to
/// the regex heuristic so nothing regresses.
fn language_for(path: &str) -> Option<Dispatch> {
    let extension = path.rsplit('.').next().filter(|ext| *ext != path)?;
    let langs = languages();
    let ts = ts_langs();
    match extension {
        "rs" => Some(Dispatch::Ast(&ts.rust)),
        "py" | "pyi" => Some(Dispatch::Ast(&ts.python)),
        // `.ts`/`.mts`/`.cts` use the TypeScript grammar; `.tsx` uses the TSX
        // dialect; the plain JavaScript extensions use the JavaScript grammar.
        "ts" | "mts" | "cts" => Some(Dispatch::Ast(&ts.typescript)),
        "tsx" => Some(Dispatch::Ast(&ts.tsx)),
        "js" | "jsx" | "mjs" | "cjs" => Some(Dispatch::Ast(&ts.javascript)),
        "go" => Some(Dispatch::Ast(&ts.go)),
        "java" | "kt" | "kts" => Some(Dispatch::Regex(&langs.java)),
        "rb" => Some(Dispatch::Regex(&langs.ruby)),
        "c" | "h" | "cc" | "cpp" | "cxx" | "hpp" | "hxx" => Some(Dispatch::Regex(&langs.c_family)),
        _ => None,
    }
}

/// Extract symbol definitions from `content` using the chosen strategy.
fn extract_symbols(dispatch: Dispatch, content: &str) -> Vec<RawSymbol> {
    match dispatch {
        Dispatch::Ast(lang) => extract_ast(lang, content),
        Dispatch::Regex(lang) => extract_regex(lang, content),
    }
}

// ---------------------------------------------------------------------------
// Tree-sitter (AST) extraction
// ---------------------------------------------------------------------------

/// A compiled tree-sitter language plus the query that captures its
/// definitions. The query's capture names encode the symbol kind: a capture
/// named `name` holds the identifier; every other capture is the definition
/// node whose name (sans the `def.` prefix) is the reported `kind`.
struct TsLang {
    language: tree_sitter::Language,
    query: Query,
}

/// Extract symbols by running the language's definition query over the AST.
///
/// A query match groups a `@name` capture (the identifier) with a `@def.<kind>`
/// capture (the enclosing definition node). The definition node gives the line
/// number and the signature (the header text up to the body), and the kind is
/// taken from the def capture's name. Because matching is over the parse tree,
/// identifiers inside strings/comments are never captured, and a signature that
/// spans multiple lines is captured in full (then collapsed/trimmed).
fn extract_ast(lang: &TsLang, content: &str) -> Vec<RawSymbol> {
    let mut parser = Parser::new();
    if parser.set_language(&lang.language).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(content, None) else {
        return Vec::new();
    };
    let src = content.as_bytes();
    let mut cursor = QueryCursor::new();
    let mut symbols = Vec::new();

    let name_idx = lang
        .query
        .capture_index_for_name("name")
        .expect("query has a @name capture");

    let mut matches = cursor.matches(&lang.query, tree.root_node(), src);
    while let Some(m) = matches.next() {
        let mut name_node: Option<Node> = None;
        let mut def_node: Option<Node> = None;
        let mut kind: &'static str = "def";
        for cap in m.captures {
            if cap.index == name_idx {
                name_node = Some(cap.node);
            } else {
                def_node = Some(cap.node);
                kind = capture_kind(&lang.query, cap.index);
            }
        }
        let (Some(name_node), Some(def_node)) = (name_node, def_node) else {
            continue;
        };
        let Ok(name) = name_node.utf8_text(src) else {
            continue;
        };
        symbols.push(RawSymbol {
            name: name.to_string(),
            kind,
            line: def_node.start_position().row + 1,
            signature: signature_for(def_node, src),
            score: 0,
        });
    }

    // Definitions can be reported out of source order across query patterns;
    // sort by line so output is deterministic and reads top-to-bottom before
    // ranking re-sorts by score (ties broken by line).
    symbols.sort_by_key(|s| s.line);
    symbols
}

/// The reported `kind` for a definition capture: its capture name with the
/// `def.` prefix stripped (so `@def.fn` reports `fn`).
fn capture_kind(query: &Query, index: u32) -> &'static str {
    let raw = &query.capture_names()[index as usize];
    let kind = raw.strip_prefix("def.").unwrap_or(raw);
    intern_kind(kind)
}

/// Intern a kind string to a `&'static str` so `RawSymbol` can keep the cheap
/// `&'static str` field shared with the regex path. The set of kinds is fixed
/// and small (it is exactly the set emitted by the queries below).
fn intern_kind(kind: &str) -> &'static str {
    match kind {
        "fn" => "fn",
        "method" => "method",
        "struct" => "struct",
        "enum" => "enum",
        "trait" => "trait",
        "type" => "type",
        "macro" => "macro",
        "mod" => "mod",
        "const" => "const",
        "static" => "static",
        "union" => "union",
        "class" => "class",
        "def" => "def",
        "function" => "function",
        "interface" => "interface",
        "func" => "func",
        "var" => "var",
        _ => "def",
    }
}

/// Build a compact, single-line signature from a definition node: the header
/// text up to the start of the body (`{`), with interior whitespace collapsed
/// and the result trimmed to the signature length cap. Multi-line headers are
/// captured in full before collapsing.
fn signature_for(node: Node, src: &[u8]) -> String {
    let full = node.utf8_text(src).unwrap_or("");
    // Cut at the first body opener so the signature is the declaration header.
    let header = match full.find('{') {
        Some(idx) => &full[..idx],
        None => full,
    };
    // Drop a trailing semicolon for declarations.
    let header = header.trim().trim_end_matches(';').trim();
    let collapsed = collapse_whitespace(header);
    trim_signature(&collapsed)
}

/// Collapse all runs of whitespace (including newlines) to single spaces.
fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The bundled tree-sitter languages and their compiled definition queries.
struct TsLangs {
    rust: TsLang,
    python: TsLang,
    javascript: TsLang,
    typescript: TsLang,
    tsx: TsLang,
    go: TsLang,
}

fn ts_langs() -> &'static TsLangs {
    static LANGS: OnceLock<TsLangs> = OnceLock::new();
    LANGS.get_or_init(build_ts_langs)
}

fn ts_lang(language: tree_sitter::Language, query_src: &str) -> TsLang {
    let query = Query::new(&language, query_src).expect("valid tree-sitter query");
    TsLang { language, query }
}

fn build_ts_langs() -> TsLangs {
    TsLangs {
        rust: ts_lang(tree_sitter_rust::LANGUAGE.into(), RUST_QUERY),
        python: ts_lang(tree_sitter_python::LANGUAGE.into(), PYTHON_QUERY),
        javascript: ts_lang(tree_sitter_javascript::LANGUAGE.into(), JAVASCRIPT_QUERY),
        typescript: ts_lang(
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            TYPESCRIPT_QUERY,
        ),
        tsx: ts_lang(
            tree_sitter_typescript::LANGUAGE_TSX.into(),
            TYPESCRIPT_QUERY,
        ),
        go: ts_lang(tree_sitter_go::LANGUAGE.into(), GO_QUERY),
    }
}

const RUST_QUERY: &str = r#"
(function_item name: (identifier) @name) @def.fn
(struct_item name: (type_identifier) @name) @def.struct
(enum_item name: (type_identifier) @name) @def.enum
(union_item name: (type_identifier) @name) @def.union
(trait_item name: (type_identifier) @name) @def.trait
(type_item name: (type_identifier) @name) @def.type
(mod_item name: (identifier) @name) @def.mod
(const_item name: (identifier) @name) @def.const
(static_item name: (identifier) @name) @def.static
(macro_definition name: (identifier) @name) @def.macro
"#;

const PYTHON_QUERY: &str = r#"
(function_definition name: (identifier) @name) @def.def
(class_definition name: (identifier) @name) @def.class
"#;

const JAVASCRIPT_QUERY: &str = r#"
(function_declaration name: (identifier) @name) @def.function
(generator_function_declaration name: (identifier) @name) @def.function
(class_declaration name: (identifier) @name) @def.class
(method_definition name: (property_identifier) @name) @def.method
(lexical_declaration
  (variable_declarator
    name: (identifier) @name
    value: [(arrow_function) (function_expression)])) @def.const
(variable_declaration
  (variable_declarator
    name: (identifier) @name
    value: [(arrow_function) (function_expression)])) @def.var
"#;

const TYPESCRIPT_QUERY: &str = r#"
(function_declaration name: (identifier) @name) @def.function
(generator_function_declaration name: (identifier) @name) @def.function
(class_declaration name: (type_identifier) @name) @def.class
(abstract_class_declaration name: (type_identifier) @name) @def.class
(interface_declaration name: (type_identifier) @name) @def.interface
(type_alias_declaration name: (type_identifier) @name) @def.type
(enum_declaration name: (identifier) @name) @def.enum
(method_definition name: (property_identifier) @name) @def.method
(lexical_declaration
  (variable_declarator
    name: (identifier) @name
    value: [(arrow_function) (function_expression)])) @def.const
"#;

const GO_QUERY: &str = r#"
(function_declaration name: (identifier) @name) @def.func
(method_declaration name: (field_identifier) @name) @def.method
(type_declaration (type_spec name: (type_identifier) @name)) @def.type
"#;

// ---------------------------------------------------------------------------
// Regex (fallback) extraction — for languages without a bundled grammar.
// ---------------------------------------------------------------------------

/// A language's definition patterns: each is `(kind, regex)` with capture group
/// 1 holding the symbol name.
struct Language {
    patterns: Vec<(&'static str, Regex)>,
}

/// Extract symbol definitions from `content` using the language's patterns.
/// Each source line yields at most one symbol (the first pattern that matches).
fn extract_regex(language: &Language, content: &str) -> Vec<RawSymbol> {
    let mut symbols = Vec::new();
    for (index, line) in content.lines().enumerate() {
        // Cheap pre-filter: skip obviously non-definition lines.
        if line.len() > 400 {
            continue;
        }
        for (kind, regex) in &language.patterns {
            if let Some(captures) = regex.captures(line)
                && let Some(name) = captures.get(1)
            {
                symbols.push(RawSymbol {
                    name: name.as_str().to_string(),
                    kind,
                    line: index + 1,
                    signature: trim_signature(line),
                    score: 0,
                });
                break;
            }
        }
    }
    symbols
}

/// Trim a source line into a compact one-line signature.
fn trim_signature(line: &str) -> String {
    let trimmed = line.trim();
    if trimmed.chars().count() > MAX_SIGNATURE_CHARS {
        let truncated: String = trimmed.chars().take(MAX_SIGNATURE_CHARS).collect();
        format!("{truncated}…")
    } else {
        trimmed.to_string()
    }
}

struct Languages {
    java: Language,
    ruby: Language,
    c_family: Language,
}

/// The per-language definition patterns, compiled once.
fn languages() -> &'static Languages {
    static LANGS: OnceLock<Languages> = OnceLock::new();
    LANGS.get_or_init(build_languages)
}

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("valid language pattern")
}

fn build_languages() -> Languages {
    Languages {
        java: Language {
            patterns: vec![
                (
                    "type",
                    re(
                        r"^\s*(?:(?:public|private|protected|static|final|abstract|sealed|open|data)\s+)*(?:class|interface|enum|record)\s+([A-Za-z_][A-Za-z0-9_]*)",
                    ),
                ),
                (
                    "fun",
                    re(
                        r"^\s*(?:(?:public|private|protected|internal|override|suspend)\s+)*fun\s+([A-Za-z_][A-Za-z0-9_]*)",
                    ),
                ),
            ],
        },
        ruby: Language {
            patterns: vec![
                (
                    "def",
                    re(r"^\s*def\s+(?:self\.)?([A-Za-z_][A-Za-z0-9_]*[!?]?)"),
                ),
                ("class", re(r"^\s*class\s+([A-Za-z_][A-Za-z0-9_:]*)")),
                ("module", re(r"^\s*module\s+([A-Za-z_][A-Za-z0-9_:]*)")),
            ],
        },
        c_family: Language {
            patterns: vec![
                (
                    "struct",
                    re(
                        r"^\s*(?:typedef\s+)?(?:struct|class|union|enum)\s+([A-Za-z_][A-Za-z0-9_]*)",
                    ),
                ),
                // A function definition: a return type, a name, an argument list,
                // and an opening brace on the same line. Conservative on purpose.
                (
                    "fn",
                    re(
                        r"^\s*(?:[A-Za-z_][\w:<>,\*&\s]*?\s+[\*&]*)([A-Za-z_][A-Za-z0-9_]*)\s*\([^;{]*\)\s*\{",
                    ),
                ),
            ],
        },
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn workspace_with(files: &[(&str, &str)]) -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        for (path, content) in files {
            let full = dir.path().join(path);
            if let Some(parent) = full.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(full, content).unwrap();
        }
        let workspace = Workspace::new(dir.path()).unwrap();
        (dir, workspace)
    }

    fn run(workspace: &Workspace, input: Value) -> Value {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let result = runtime
            .block_on(RepoMapTool.execute(workspace, input))
            .unwrap();
        result.content().clone()
    }

    /// Collect every (name, kind) pair across all files in a map result.
    fn name_kinds(map: &Value) -> Vec<(String, String)> {
        map["files"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|f| f["symbols"].as_array().unwrap().clone())
            .map(|s| {
                (
                    s["name"].as_str().unwrap().to_string(),
                    s["kind"].as_str().unwrap_or("").to_string(),
                )
            })
            .collect()
    }

    #[test]
    fn extracts_rust_symbols() {
        let (_dir, ws) = workspace_with(&[(
            "src/lib.rs",
            "pub fn alpha() {}\npub struct Beta;\nenum Gamma { A }\npub trait Delta {}\n",
        )]);
        let map = run(&ws, json!({}));
        let files = map["files"].as_array().unwrap();
        assert_eq!(files.len(), 1);
        let kinds: Vec<&str> = files[0]["symbols"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["kind"].as_str().unwrap())
            .collect();
        assert!(kinds.contains(&"fn"));
        assert!(kinds.contains(&"struct"));
        assert!(kinds.contains(&"enum"));
        assert!(kinds.contains(&"trait"));
    }

    #[test]
    fn rust_ast_ignores_definitions_in_strings_and_comments() {
        // The OLD per-line regex would have extracted `ghost` (in a comment) and
        // `phantom` (in a string literal). The AST parser only sees the real
        // `real` definition.
        let (_dir, ws) = workspace_with(&[(
            "src/lib.rs",
            "// pub fn ghost() {}\nfn host() { let s = \"pub fn phantom() {}\"; }\npub fn real() {}\n",
        )]);
        let map = run(&ws, json!({}));
        let names: Vec<String> = name_kinds(&map).into_iter().map(|(n, _)| n).collect();
        assert!(
            names.contains(&"real".to_string()),
            "missing real: {names:?}"
        );
        assert!(
            !names.contains(&"ghost".to_string()),
            "comment def extracted: {names:?}"
        );
        assert!(
            !names.contains(&"phantom".to_string()),
            "string def extracted: {names:?}"
        );
    }

    #[test]
    fn rust_ast_captures_multiline_signature() {
        // A function whose signature spans several lines: the AST captures the
        // whole header (collapsed to one line). A per-line regex would only see
        // the first line `pub fn wide(`.
        let (_dir, ws) = workspace_with(&[(
            "src/lib.rs",
            "pub fn wide(\n    a: u32,\n    b: u32,\n) -> u32 {\n    a + b\n}\n",
        )]);
        let map = run(&ws, json!({}));
        let sig = map["files"][0]["symbols"][0]["signature"].as_str().unwrap();
        assert_eq!(sig, "pub fn wide( a: u32, b: u32, ) -> u32");
    }

    #[test]
    fn extracts_python_symbols_via_ast() {
        let (_dir, ws) = workspace_with(&[(
            "a.py",
            "def py_func():\n    pass\nclass PyClass:\n    def method(self):\n        pass\n",
        )]);
        let map = run(&ws, json!({}));
        let nks = name_kinds(&map);
        assert!(nks.contains(&("py_func".to_string(), "def".to_string())));
        assert!(nks.contains(&("PyClass".to_string(), "class".to_string())));
        // The method inside the class is also captured by the AST.
        assert!(nks.iter().any(|(n, _)| n == "method"));
    }

    #[test]
    fn extracts_go_symbols_via_ast() {
        let (_dir, ws) = workspace_with(&[(
            "b.go",
            "package main\nfunc GoFunc() {}\ntype GoType struct{}\nfunc (g GoType) Method() {}\n",
        )]);
        let map = run(&ws, json!({}));
        let nks = name_kinds(&map);
        assert!(nks.contains(&("GoFunc".to_string(), "func".to_string())));
        assert!(nks.contains(&("GoType".to_string(), "type".to_string())));
        assert!(nks.contains(&("Method".to_string(), "method".to_string())));
    }

    #[test]
    fn extracts_typescript_symbols_via_ast() {
        let (_dir, ws) = workspace_with(&[(
            "c.ts",
            "export function tsFunc() {}\nexport class TsClass {}\nexport interface TsIface { x: number }\nexport type TsAlias = string\nexport const arrow = () => 1\n",
        )]);
        let map = run(&ws, json!({}));
        let nks = name_kinds(&map);
        assert!(nks.contains(&("tsFunc".to_string(), "function".to_string())));
        assert!(nks.contains(&("TsClass".to_string(), "class".to_string())));
        assert!(nks.contains(&("TsIface".to_string(), "interface".to_string())));
        assert!(nks.contains(&("TsAlias".to_string(), "type".to_string())));
        assert!(nks.contains(&("arrow".to_string(), "const".to_string())));
    }

    #[test]
    fn extracts_javascript_symbols_via_ast() {
        let (_dir, ws) = workspace_with(&[(
            "c.js",
            "export function jsFunc() {}\nexport class JsClass {}\nconst arrow = () => 1\n",
        )]);
        let map = run(&ws, json!({}));
        let nks = name_kinds(&map);
        assert!(nks.contains(&("jsFunc".to_string(), "function".to_string())));
        assert!(nks.contains(&("JsClass".to_string(), "class".to_string())));
        assert!(nks.contains(&("arrow".to_string(), "const".to_string())));
    }

    #[test]
    fn regex_fallback_still_works_for_ruby() {
        // Ruby has no bundled grammar, so it must still extract via the regex
        // fallback path.
        let (_dir, ws) = workspace_with(&[(
            "a.rb",
            "class RbClass\n  def rb_method\n  end\nend\nmodule RbMod\nend\n",
        )]);
        let map = run(&ws, json!({}));
        let nks = name_kinds(&map);
        assert!(nks.contains(&("RbClass".to_string(), "class".to_string())));
        assert!(nks.contains(&("rb_method".to_string(), "def".to_string())));
        assert!(nks.contains(&("RbMod".to_string(), "module".to_string())));
    }

    #[test]
    fn ranks_referenced_symbols_higher() {
        // `helper` is called many times across files; `lonely` never is.
        let (_dir, ws) = workspace_with(&[
            ("a.rs", "pub fn helper() {}\npub fn lonely() {}\n"),
            ("b.rs", "fn use_it() { helper(); helper(); helper(); }\n"),
        ]);
        let map = run(&ws, json!({}));
        let files = map["files"].as_array().unwrap();
        // a.rs defines the referenced symbol, so it ranks first.
        assert_eq!(files[0]["path"], "a.rs");
        let symbols = files[0]["symbols"].as_array().unwrap();
        assert_eq!(symbols[0]["name"], "helper");
        assert!(
            symbols[0]["references"].as_u64().unwrap() > symbols[1]["references"].as_u64().unwrap()
        );
    }

    #[test]
    fn supports_multiple_languages() {
        let (_dir, ws) = workspace_with(&[
            (
                "a.py",
                "def py_func():\n    pass\nclass PyClass:\n    pass\n",
            ),
            (
                "b.go",
                "package main\nfunc GoFunc() {}\ntype GoType struct{}\n",
            ),
            (
                "c.ts",
                "export function tsFunc() {}\nexport class TsClass {}\n",
            ),
        ]);
        let map = run(&ws, json!({}));
        let all_names: Vec<String> = map["files"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|f| f["symbols"].as_array().unwrap().clone())
            .map(|s| s["name"].as_str().unwrap().to_string())
            .collect();
        for expected in [
            "py_func", "PyClass", "GoFunc", "GoType", "tsFunc", "TsClass",
        ] {
            assert!(
                all_names.contains(&expected.to_string()),
                "missing {expected}"
            );
        }
    }

    #[test]
    fn skips_non_source_and_ignored() {
        let (_dir, ws) = workspace_with(&[
            ("src/main.rs", "fn main() {}\n"),
            ("README.md", "# not code\n"),
            ("target/gen.rs", "fn generated() {}\n"),
        ]);
        let map = run(&ws, json!({}));
        let paths: Vec<&str> = map["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["path"].as_str().unwrap())
            .collect();
        assert_eq!(paths, vec!["src/main.rs"]);
        assert_eq!(map["files_scanned"], 1);
    }

    #[test]
    fn skips_oversized_source_files_via_size_stat() {
        // An oversized source file is skipped by the metadata size-gate without
        // being read into memory; only the in-limit file is scanned.
        let (dir, ws) = workspace_with(&[("src/main.rs", "fn main() {}\n")]);
        let mut big = vec![b'x'; (MAX_SOURCE_FILE_BYTES + 16) as usize];
        big[..13].copy_from_slice(b"fn huge() {}\n");
        fs::write(dir.path().join("src/big.rs"), &big).unwrap();

        let map = run(&ws, json!({}));
        let paths: Vec<&str> = map["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["path"].as_str().unwrap())
            .collect();
        assert_eq!(paths, vec!["src/main.rs"]);
        assert_eq!(map["files_scanned"], 1);
    }

    #[test]
    fn glob_filter_scopes_the_map() {
        let (_dir, ws) =
            workspace_with(&[("src/a.rs", "fn a() {}\n"), ("tests/b.rs", "fn b() {}\n")]);
        let map = run(&ws, json!({ "glob": "src/**/*.rs" }));
        let paths: Vec<&str> = map["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["path"].as_str().unwrap())
            .collect();
        assert_eq!(paths, vec!["src/a.rs"]);
    }

    #[test]
    fn concise_lists_symbol_names_only() {
        let (_dir, ws) = workspace_with(&[("src/lib.rs", "pub fn alpha() {}\npub struct Beta;\n")]);
        let map = run(&ws, json!({ "verbosity": "concise" }));
        let symbols = map["files"][0]["symbols"].as_array().unwrap();
        for symbol in symbols {
            assert!(symbol["name"].is_string());
            // Concise drops signatures and the other per-symbol detail.
            assert!(symbol.get("signature").is_none());
            assert!(symbol.get("kind").is_none());
            assert!(symbol.get("line").is_none());
            assert!(symbol.get("references").is_none());
        }
        let names: Vec<&str> = symbols
            .iter()
            .map(|s| s["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"alpha"));
        assert!(names.contains(&"Beta"));
    }

    #[test]
    fn default_equals_detailed_with_signatures() {
        // Omitted verbosity == detailed == the historical shape (full
        // signatures and per-symbol detail).
        let (_dir, ws) = workspace_with(&[("src/lib.rs", "pub fn alpha() {}\npub struct Beta;\n")]);
        let default = run(&ws, json!({}));
        let detailed = run(&ws, json!({ "verbosity": "detailed" }));
        assert_eq!(default, detailed);
        let first = &default["files"][0]["symbols"][0];
        assert!(first["signature"].is_string());
        assert!(first["kind"].is_string());
        assert!(first["line"].is_number());
        assert!(first["references"].is_number());
    }

    #[test]
    fn max_files_truncates_and_flags() {
        let (_dir, ws) = workspace_with(&[
            ("a.rs", "fn a() {}\n"),
            ("b.rs", "fn b() {}\n"),
            ("c.rs", "fn c() {}\n"),
        ]);
        let map = run(&ws, json!({ "max_files": 2 }));
        assert_eq!(map["files"].as_array().unwrap().len(), 2);
        assert_eq!(map["truncated_files"], true);
    }
}
