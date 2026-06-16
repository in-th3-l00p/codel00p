//! `repo_map`: a ranked, compact map of the code symbols in the workspace.
//!
//! This is the navigation aid mature coding agents (Aider's repo map, Claude
//! Code's codebase understanding) lean on: instead of reading whole files, the
//! model gets the most *important* definitions across the repo — functions,
//! types, classes — ranked so the symbols other code depends on most surface
//! first.
//!
//! Symbol extraction is a dependency-light, multi-language heuristic (per-
//! language regexes over definition lines), not a full parse. Importance is a
//! cheap proxy for Aider's PageRank: a symbol's score is how often its name is
//! referenced across the whole repo, and a file's score is the sum of its
//! symbols' scores. It is deterministic and good enough to point the model at
//! the right files fast; for exact navigation it pairs with `grep` / `read_file`.

use std::{collections::HashMap, fs, sync::OnceLock};

use async_trait::async_trait;
use codel00p_protocol::PermissionScope;
use regex::Regex;
use serde_json::{Value, json};

use crate::{
    errors::HarnessError,
    tool_result::ToolResult,
    tools::{Tool, optional_string},
    walk::{GlobMatcher, walk_files},
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
         Java, Ruby, and C/C++. Pair with `grep`/`read_file` for exact navigation."
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
                }
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

        let root = workspace.resolve(path)?;
        let mut relatives = Vec::new();
        walk_files(workspace.root(), &root, include_ignored, &mut |relative| {
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
            let absolute = workspace.resolve(relative)?;
            if fs::metadata(&absolute)
                .map(|meta| meta.len() > MAX_SOURCE_FILE_BYTES)
                .unwrap_or(true)
            {
                continue;
            }
            let Ok(content) = fs::read_to_string(&absolute) else {
                continue;
            };
            files_scanned += 1;

            for ident in identifier_regex().find_iter(&content) {
                *reference_counts
                    .entry(ident.as_str().to_string())
                    .or_insert(0) += 1;
            }

            let language = language_for(relative).expect("filtered to known languages");
            let symbols = extract_symbols(language, &content);
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
                        json!({
                            "name": symbol.name,
                            "kind": symbol.kind,
                            "line": symbol.line,
                            "signature": symbol.signature,
                            "references": symbol.score,
                        })
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

/// A language's definition patterns: each is `(kind, regex)` with capture group
/// 1 holding the symbol name.
struct Language {
    patterns: Vec<(&'static str, Regex)>,
}

/// Map a file path to its language by extension, or `None` if unsupported.
fn language_for(path: &str) -> Option<&'static Language> {
    let extension = path.rsplit('.').next().filter(|ext| *ext != path)?;
    let langs = languages();
    match extension {
        "rs" => Some(&langs.rust),
        "py" | "pyi" => Some(&langs.python),
        "js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" => Some(&langs.javascript),
        "go" => Some(&langs.go),
        "java" | "kt" | "kts" => Some(&langs.java),
        "rb" => Some(&langs.ruby),
        "c" | "h" | "cc" | "cpp" | "cxx" | "hpp" | "hxx" => Some(&langs.c_family),
        _ => None,
    }
}

/// Extract symbol definitions from `content` using the language's patterns.
/// Each source line yields at most one symbol (the first pattern that matches).
fn extract_symbols(language: &Language, content: &str) -> Vec<RawSymbol> {
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
    rust: Language,
    python: Language,
    javascript: Language,
    go: Language,
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
        rust: Language {
            patterns: vec![
                (
                    "fn",
                    re(
                        r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:default\s+)?(?:const\s+)?(?:async\s+)?(?:unsafe\s+)?(?:extern\s+\x22[^\x22]*\x22\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)",
                    ),
                ),
                (
                    "struct",
                    re(r"^\s*(?:pub(?:\([^)]*\))?\s+)?struct\s+([A-Za-z_][A-Za-z0-9_]*)"),
                ),
                (
                    "enum",
                    re(r"^\s*(?:pub(?:\([^)]*\))?\s+)?enum\s+([A-Za-z_][A-Za-z0-9_]*)"),
                ),
                (
                    "trait",
                    re(
                        r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:unsafe\s+)?trait\s+([A-Za-z_][A-Za-z0-9_]*)",
                    ),
                ),
                (
                    "type",
                    re(r"^\s*(?:pub(?:\([^)]*\))?\s+)?type\s+([A-Za-z_][A-Za-z0-9_]*)"),
                ),
                ("macro", re(r"^\s*macro_rules!\s+([A-Za-z_][A-Za-z0-9_]*)")),
            ],
        },
        python: Language {
            patterns: vec![
                (
                    "def",
                    re(r"^\s*(?:async\s+)?def\s+([A-Za-z_][A-Za-z0-9_]*)"),
                ),
                ("class", re(r"^\s*class\s+([A-Za-z_][A-Za-z0-9_]*)")),
            ],
        },
        javascript: Language {
            patterns: vec![
                (
                    "function",
                    re(
                        r"^\s*(?:export\s+)?(?:default\s+)?(?:async\s+)?function\s*\*?\s+([A-Za-z_$][A-Za-z0-9_$]*)",
                    ),
                ),
                (
                    "class",
                    re(
                        r"^\s*(?:export\s+)?(?:default\s+)?(?:abstract\s+)?class\s+([A-Za-z_$][A-Za-z0-9_$]*)",
                    ),
                ),
                (
                    "const",
                    re(
                        r"^\s*(?:export\s+)?(?:const|let|var)\s+([A-Za-z_$][A-Za-z0-9_$]*)\s*=\s*(?:async\s*)?\(?[^=]*=>",
                    ),
                ),
                (
                    "interface",
                    re(r"^\s*(?:export\s+)?interface\s+([A-Za-z_$][A-Za-z0-9_$]*)"),
                ),
                (
                    "type",
                    re(r"^\s*(?:export\s+)?type\s+([A-Za-z_$][A-Za-z0-9_$]*)\s*="),
                ),
            ],
        },
        go: Language {
            patterns: vec![
                (
                    "func",
                    re(r"^\s*func\s+(?:\([^)]*\)\s*)?([A-Za-z_][A-Za-z0-9_]*)"),
                ),
                ("type", re(r"^\s*type\s+([A-Za-z_][A-Za-z0-9_]*)")),
            ],
        },
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
            ("b.go", "func GoFunc() {}\ntype GoType struct{}\n"),
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
