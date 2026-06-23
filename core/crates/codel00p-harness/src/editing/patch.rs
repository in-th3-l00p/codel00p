//! The pure find/replace engine behind `apply_patch`.
//!
//! Given a file's current contents and a `find`/`replace` pair, locate the
//! occurrence(s) and produce the patched string — preserving every byte outside
//! the matched region. There is no I/O here: the engine is a deterministic string
//! function, which is what makes it cheap to unit-test exhaustively (see below)
//! and lets the tool layer (`super::apply_patch`) stay a thin orchestration shell.
//!
//! Matching tries the exact substring first, then three tolerant fallbacks for
//! the drift smaller models introduce (line endings, trailing whitespace, a
//! uniform indentation shift). The first strategy that matches wins.

/// Result of successfully applying a single change to a file's contents.
#[derive(Debug)]
pub(super) struct ChangeOutcome {
    pub(super) patched: String,
    pub(super) replacements: usize,
    pub(super) strategy: &'static str,
}

/// A located occurrence of the `find` text within the original contents,
/// expressed as a byte range plus the exact replacement text to splice in.
struct Located {
    /// Half-open byte range `[start, end)` in the original contents.
    range: std::ops::Range<usize>,
    /// The text to write in place of `range`. Usually the caller's `replace`,
    /// but indentation-tolerant matching re-indents it to the file's actual
    /// leading whitespace so surrounding formatting is preserved.
    replacement: String,
}

/// A matching strategy: locate every occurrence of `find` in the original and
/// return the byte ranges (with replacement text) to splice.
type LocateFn = fn(&str, &str, &str) -> Vec<Located>;

/// Try, in order, the exact fast path then the tolerant fallbacks, and apply
/// the first strategy that yields at least one match.
///
/// Returns the patched contents on success, or a human-readable, actionable
/// error message (without the file path, which the caller prepends).
pub(super) fn apply_change(
    original: &str,
    find: &str,
    replace: &str,
    replace_all: bool,
) -> Result<ChangeOutcome, String> {
    // Ordered strategies. Each returns the byte ranges (with re-indented
    // replacement text) it matched; the first non-empty wins.
    let strategies: [(&'static str, LocateFn); 4] = [
        ("exact", locate_exact),
        ("line-ending", locate_line_ending),
        ("trailing-whitespace", locate_trailing_whitespace),
        ("indentation", locate_indentation),
    ];

    for (strategy, locate) in strategies {
        let matches = locate(original, find, replace);
        if matches.is_empty() {
            continue;
        }
        if matches.len() > 1 && !replace_all {
            return Err(format!(
                "found {} matches for the `find` text via {strategy} matching; \
                 add surrounding context so it is unique, or set `replace_all` \
                 to true to replace every occurrence",
                matches.len()
            ));
        }
        let replacements = matches.len();
        let patched = splice(original, matches);
        return Ok(ChangeOutcome {
            patched,
            replacements,
            strategy,
        });
    }

    Err(not_found_hint(original, find))
}

/// Replace each located byte range with its replacement text, preserving every
/// byte outside the matched regions exactly. Ranges are sorted/non-overlapping.
fn splice(original: &str, mut matches: Vec<Located>) -> String {
    matches.sort_by_key(|m| m.range.start);
    let mut out = String::with_capacity(original.len());
    let mut cursor = 0usize;
    for located in matches {
        out.push_str(&original[cursor..located.range.start]);
        out.push_str(&located.replacement);
        cursor = located.range.end;
    }
    out.push_str(&original[cursor..]);
    out
}

/// Exact substring matching — the fast path. Preserves prior behaviour.
fn locate_exact(original: &str, find: &str, replace: &str) -> Vec<Located> {
    let mut matches = Vec::new();
    let mut start = 0usize;
    while let Some(found) = original[start..].find(find) {
        let abs = start + found;
        matches.push(Located {
            range: abs..abs + find.len(),
            replacement: replace.to_string(),
        });
        start = abs + find.len();
    }
    matches
}

/// Match line-by-line, ignoring trailing whitespace on every line of both the
/// `find` block and the file. Useful when the model omits or adds trailing
/// spaces. The replacement text is used verbatim.
fn locate_trailing_whitespace(original: &str, find: &str, replace: &str) -> Vec<Located> {
    locate_by_line(original, find, replace, |line| line.trim_end().to_string())
}

/// Match after normalising line endings (CRLF -> LF) on both sides, so a CRLF
/// file can be edited with an LF `find` block (and vice versa).
fn locate_line_ending(original: &str, find: &str, replace: &str) -> Vec<Located> {
    // Only meaningful when the two sides disagree on line endings; otherwise the
    // exact strategy would already have matched.
    locate_by_line(original, find, replace, |line| {
        line.trim_end_matches('\r').to_string()
    })
}

/// Indentation-tolerant matching: match the `find` block ignoring a *uniform*
/// leading-whitespace shift, then re-indent the replacement by the file's actual
/// indentation so the surrounding formatting is preserved.
fn locate_indentation(original: &str, find: &str, replace: &str) -> Vec<Located> {
    let find_lines: Vec<&str> = split_keep_ends(find);
    if find_lines.is_empty() {
        return Vec::new();
    }
    let orig_lines = split_keep_ends(original);

    // Offsets of each original line's start byte.
    let mut offsets = Vec::with_capacity(orig_lines.len() + 1);
    let mut acc = 0usize;
    for line in &orig_lines {
        offsets.push(acc);
        acc += line.len();
    }
    offsets.push(acc);

    let find_keys: Vec<String> = find_lines
        .iter()
        .map(|l| strip_eol(l).trim_start().to_string())
        .collect();

    let mut matches = Vec::new();
    let window = find_lines.len();
    if window == 0 || window > orig_lines.len() {
        return matches;
    }

    let mut i = 0usize;
    while i + window <= orig_lines.len() {
        let mut ok = true;
        for (k, find_key) in find_keys.iter().enumerate() {
            let orig_line = strip_eol(orig_lines[i + k]);
            if orig_line.trim_start() != *find_key {
                ok = false;
                break;
            }
        }
        if ok {
            let start = offsets[i];
            let end = offsets[i + window];
            // Preserve the file's actual indentation: re-indent the replacement
            // by the leading whitespace of the first matched original line.
            let base_indent = leading_whitespace(strip_eol(orig_lines[i]));
            let replacement = reindent(replace, base_indent);
            matches.push(Located {
                range: start..end,
                replacement,
            });
            i += window;
        } else {
            i += 1;
        }
    }
    matches
}

/// Shared line-window matcher: compares the `find` block against the file with a
/// per-line normalising function applied to both sides. The replacement text is
/// used verbatim (line-ending / trailing-whitespace tolerance does not reshape
/// the replacement).
fn locate_by_line(
    original: &str,
    find: &str,
    replace: &str,
    normalize: impl Fn(&str) -> String,
) -> Vec<Located> {
    let find_lines: Vec<&str> = split_keep_ends(find);
    if find_lines.is_empty() {
        return Vec::new();
    }
    let orig_lines = split_keep_ends(original);
    let window = find_lines.len();
    if window > orig_lines.len() {
        return Vec::new();
    }

    let mut offsets = Vec::with_capacity(orig_lines.len() + 1);
    let mut acc = 0usize;
    for line in &orig_lines {
        offsets.push(acc);
        acc += line.len();
    }
    offsets.push(acc);

    let find_keys: Vec<String> = find_lines.iter().map(|l| normalize(strip_eol(l))).collect();

    let mut matches = Vec::new();
    let mut i = 0usize;
    while i + window <= orig_lines.len() {
        let mut ok = true;
        for (k, find_key) in find_keys.iter().enumerate() {
            if normalize(strip_eol(orig_lines[i + k])) != *find_key {
                ok = false;
                break;
            }
        }
        if ok {
            let start = offsets[i];
            let end = offsets[i + window];
            matches.push(Located {
                range: start..end,
                replacement: replace.to_string(),
            });
            i += window;
        } else {
            i += 1;
        }
    }
    matches
}

/// Split into lines while keeping each line's trailing newline (so byte offsets
/// reconstruct the original exactly). A trailing newline does not produce a
/// final empty element.
fn split_keep_ends(text: &str) -> Vec<&str> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut lines = Vec::new();
    let mut start = 0usize;
    let bytes = text.as_bytes();
    for (idx, &byte) in bytes.iter().enumerate() {
        if byte == b'\n' {
            lines.push(&text[start..=idx]);
            start = idx + 1;
        }
    }
    if start < text.len() {
        lines.push(&text[start..]);
    }
    lines
}

/// Strip a trailing `\n` and optional preceding `\r` from a line slice.
fn strip_eol(line: &str) -> &str {
    line.strip_suffix('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .unwrap_or(line)
}

/// The leading-whitespace prefix of a line.
fn leading_whitespace(line: &str) -> &str {
    let end = line
        .find(|c: char| !c.is_whitespace())
        .unwrap_or(line.len());
    &line[..end]
}

/// Re-indent every non-empty line of `replace` so its least-indented line sits
/// at `base_indent`, preserving relative indentation between lines.
fn reindent(replace: &str, base_indent: &str) -> String {
    let lines: Vec<&str> = split_keep_ends(replace);
    if lines.is_empty() {
        return replace.to_string();
    }

    let common = lines
        .iter()
        .filter(|l| !strip_eol(l).trim().is_empty())
        .map(|l| leading_whitespace(strip_eol(l)).len())
        .min()
        .unwrap_or(0);

    let mut out = String::with_capacity(replace.len() + base_indent.len() * lines.len());
    for line in lines {
        let body = strip_eol(line);
        let eol = &line[body.len()..];
        if body.trim().is_empty() {
            out.push_str(body);
        } else {
            out.push_str(base_indent);
            out.push_str(&body[common..]);
        }
        out.push_str(eol);
    }
    out
}

/// Build an actionable not-found message, pointing at the closest near-miss line
/// when one exists so the model can self-correct.
fn not_found_hint(original: &str, find: &str) -> String {
    let find_first = find.lines().next().unwrap_or("").trim();
    if find_first.is_empty() {
        return "find text was not present".to_string();
    }

    // Look for a line that only differs by whitespace.
    let collapsed_target: String = find_first.split_whitespace().collect::<Vec<_>>().join(" ");
    for line in original.lines() {
        let collapsed: String = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if collapsed == collapsed_target && line.trim() != find_first {
            return format!(
                "find text was not present; a line differs only in whitespace: {:?}",
                line
            );
        }
    }

    // Otherwise surface the closest line by a cheap similarity heuristic.
    let mut best: Option<(usize, &str)> = None;
    for line in original.lines() {
        let score = shared_prefix_len(line.trim(), find_first);
        if score == 0 {
            continue;
        }
        match best {
            Some((best_score, _)) if score <= best_score => {}
            _ => best = Some((score, line)),
        }
    }

    match best {
        Some((_, line)) => format!(
            "find text was not present; closest line is: {:?}",
            line.trim()
        ),
        None => "find text was not present".to_string(),
    }
}

/// Length of the shared leading prefix between two strings (cheap near-miss
/// heuristic).
fn shared_prefix_len(a: &str, b: &str) -> usize {
    a.chars().zip(b.chars()).take_while(|(x, y)| x == y).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(original: &str, find: &str, replace: &str) -> Result<ChangeOutcome, String> {
        apply_change(original, find, replace, false)
    }

    #[test]
    fn exact_match_is_the_fast_path_and_preserves_surrounding_bytes() {
        let out = apply("let a = 1;\nlet b = 2;\n", "a = 1", "a = 99").unwrap();
        assert_eq!(out.strategy, "exact");
        assert_eq!(out.replacements, 1);
        assert_eq!(out.patched, "let a = 99;\nlet b = 2;\n");
    }

    #[test]
    fn multiple_matches_are_rejected_unless_replace_all() {
        let err = apply("x x x", "x", "y").unwrap_err();
        assert!(err.contains("found 3 matches"), "{err}");
        let out = apply_change("x x x", "x", "y", true).unwrap();
        assert_eq!(out.replacements, 3);
        assert_eq!(out.patched, "y y y");
    }

    #[test]
    fn falls_back_to_trailing_whitespace_then_crlf_tolerance() {
        // Trailing space in the file the `find` block omits. The matched window
        // spans both whole lines (including the final newline), so the verbatim
        // replacement defines the result exactly.
        let out = apply("foo \nbar\n", "foo\nbar", "baz\nqux").unwrap();
        assert_eq!(out.strategy, "trailing-whitespace");
        assert_eq!(out.patched, "baz\nqux");

        // CRLF file edited with an LF find block.
        let out = apply("a\r\nb\r\n", "a\nb", "c\nd").unwrap();
        assert_eq!(out.strategy, "line-ending");
    }

    #[test]
    fn indentation_tolerant_match_reindents_replacement_to_file() {
        // `find` is indented more than the file, so it is not an exact substring
        // and only the indentation strategy matches. The replacement is then
        // re-indented to the file's actual (four-space) indentation.
        let out = apply("    return x;\n", "        return x;", "return y;").unwrap();
        assert_eq!(out.strategy, "indentation");
        // The line-window match spans the whole line (including its newline), and
        // the verbatim replacement carries none, so the result is re-indented to
        // four spaces with no trailing newline.
        assert_eq!(out.patched, "    return y;");
    }

    #[test]
    fn not_found_hint_points_at_the_closest_line() {
        let err = apply("let answer = 42;\n", "let answr = 42;", "x").unwrap_err();
        assert!(err.contains("find text was not present"), "{err}");
        assert!(err.contains("closest line"), "{err}");
    }

    #[test]
    fn deletion_via_empty_replacement_is_supported() {
        let out = apply("keep\nremove\n", "remove\n", "").unwrap();
        assert_eq!(out.patched, "keep\n");
    }
}
