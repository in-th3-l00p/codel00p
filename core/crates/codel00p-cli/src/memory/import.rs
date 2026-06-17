use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

use codel00p_memory::{MemoryCandidateInput, MemoryError, MemoryRepository};
use codel00p_protocol::{MemoryKind, MemorySource};

use crate::config::{CliConfig, CliResult, open_memory_store, required_value};

use super::{json::memory_record_json, parse::parse_kind};

/// Split markdown text on top-level `# ` headings. Each section includes the
/// heading line and the body up to (but not including) the next heading.
/// Sections whose body (everything after the heading line) is empty/whitespace
/// are skipped.
fn split_sections(text: &str) -> Vec<String> {
    let mut sections: Vec<String> = Vec::new();
    let mut current: Option<(String, String)> = None; // (heading, body)

    for line in text.lines() {
        if line.starts_with("# ") {
            if let Some((heading, body)) = current.take()
                && !body.trim().is_empty()
            {
                sections.push(format!("{heading}\n{body}"));
            }
            current = Some((line.to_string(), String::new()));
        } else if let Some((_, ref mut body)) = current {
            body.push_str(line);
            body.push('\n');
        }
        // lines before the first heading are ignored in split mode
    }
    if let Some((heading, body)) = current
        && !body.trim().is_empty()
    {
        sections.push(format!("{heading}\n{body}"));
    }
    sections
}

/// Derive a short deterministic ID from a file path + section index.
/// Uses a 32-bit FNV-style hash via `DefaultHasher` on `"<abs_path>#<index>"`.
fn derive_id(abs_path: &str, index: usize) -> String {
    let key = format!("{abs_path}#{index}");
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    let h = hasher.finish();
    format!("import-{h:016x}")
}

pub(super) fn memory_import(config: CliConfig, args: &[String]) -> CliResult<String> {
    let Some(path_arg) = args.first() else {
        return Err("missing file path".to_string());
    };

    let mut kind = MemoryKind::Architecture;
    let mut tags: Vec<String> = Vec::new();
    let mut split = false;
    let mut json_output = false;
    let mut index = 1;

    while index < args.len() {
        match args[index].as_str() {
            "--kind" => {
                kind = parse_kind(&required_value(args, index, "--kind")?)?;
                index += 2;
            }
            "--tag" => {
                tags.push(required_value(args, index, "--tag")?);
                index += 2;
            }
            "--split-sections" => {
                split = true;
                index += 1;
            }
            "--project" => {
                // consumed (project already resolved via global flags)
                let _ = required_value(args, index, "--project")?;
                index += 2;
            }
            "--json" => {
                json_output = true;
                index += 1;
            }
            flag => return Err(format!("unknown memory import option: {flag}")),
        }
    }

    let path = PathBuf::from(path_arg);
    let abs_path = path
        .canonicalize()
        .map_err(|e| format!("cannot resolve path '{}': {e}", path.display()))?;
    let abs_path_str = abs_path.to_string_lossy().into_owned();

    let text = std::fs::read_to_string(&abs_path)
        .map_err(|e| format!("cannot read '{}': {e}", abs_path.display()))?;

    let sections: Vec<(usize, String)> = if split {
        split_sections(&text).into_iter().enumerate().collect()
    } else {
        vec![(0, text)]
    };

    let mut store = open_memory_store(&config)?;
    let mut created = Vec::new();
    let mut skipped = Vec::new();

    for (idx, content) in sections {
        if content.trim().is_empty() {
            continue;
        }
        let id = derive_id(&abs_path_str, idx);
        let source = MemorySource::import(&abs_path_str);
        let mut input =
            MemoryCandidateInput::new(&id, config.project.clone(), kind, content.trim(), source);
        for tag in &tags {
            input = input.with_tag(tag.clone());
        }
        match store.create_candidate(input) {
            Ok(record) => created.push(record),
            Err(MemoryError::MemoryAlreadyExists { id: dup_id }) => {
                skipped.push(dup_id);
            }
            Err(e) => return Err(e.to_string()),
        }
    }

    if json_output {
        let items: Vec<_> = created.iter().map(memory_record_json).collect();
        return serde_json::to_string(&items).map_err(|e| e.to_string());
    }

    let mut output = String::new();
    output.push_str(&format!(
        "imported {} candidate(s) from {abs_path_str}\n",
        created.len()
    ));
    for record in &created {
        output.push_str(&format!("  created\t{}\n", record.entry().id()));
    }
    for id in &skipped {
        output.push_str(&format!("  skipped\t{id} (already exists)\n"));
    }
    Ok(output)
}
