//! Extraction of explicit `remember ...:` directives into review candidates.

use codel00p_protocol::MemoryKind;

use crate::{MemoryCandidateInput, MemoryError, MemoryExtractionInput, util::non_empty_filter};

pub trait MemoryCandidateExtractor {
    fn extract(
        &self,
        input: MemoryExtractionInput,
    ) -> Result<Vec<MemoryCandidateInput>, MemoryError>;
}

#[derive(Clone, Debug, Default)]
pub struct ExplicitMemoryExtractor;

impl MemoryCandidateExtractor for ExplicitMemoryExtractor {
    fn extract(
        &self,
        input: MemoryExtractionInput,
    ) -> Result<Vec<MemoryCandidateInput>, MemoryError> {
        let mut candidates = Vec::new();
        for line in input.text().lines() {
            let Some((kind, directive_tags, content)) = parse_remember_directive(line) else {
                continue;
            };

            let id = format!(
                "memory-candidate-{}-{}-{}",
                input.source().session_id().as_str(),
                input.source().turn_id().as_str(),
                candidates.len() + 1
            );
            let mut candidate = MemoryCandidateInput::new(
                id,
                input.project().clone(),
                kind,
                content,
                input.source().clone(),
            );
            for tag in input.tags() {
                candidate = candidate.with_tag(tag);
            }
            for tag in directive_tags {
                candidate = candidate.with_tag(tag);
            }
            candidates.push(candidate);
        }

        Ok(candidates)
    }
}

fn parse_remember_directive(line: &str) -> Option<(MemoryKind, Vec<String>, String)> {
    let line = line.trim();
    let rest = line.strip_prefix("remember")?.trim_start();
    let (header, content) = rest.split_once(':')?;
    let content = non_empty_filter(content.to_string())?;
    let header = header.trim();
    let kind = parse_directive_kind(header)?;
    let tags = parse_directive_tags(header);

    Some((kind, tags, content))
}

fn parse_directive_kind(header: &str) -> Option<MemoryKind> {
    if header.is_empty() {
        return Some(MemoryKind::Decision);
    }

    let kind = header
        .split_once('[')
        .map(|(kind, _)| kind)
        .unwrap_or(header)
        .trim();

    if kind.is_empty() {
        Some(MemoryKind::Decision)
    } else {
        memory_kind_from_label(kind)
    }
}

fn parse_directive_tags(header: &str) -> Vec<String> {
    let Some((_, raw_tags)) = header.split_once('[') else {
        return Vec::new();
    };
    let Some((raw_tags, _)) = raw_tags.split_once(']') else {
        return Vec::new();
    };

    raw_tags
        .split(',')
        .filter_map(|tag| non_empty_filter(tag.to_string()))
        .collect()
}

fn memory_kind_from_label(label: &str) -> Option<MemoryKind> {
    match label.trim().to_ascii_lowercase().as_str() {
        "architecture" => Some(MemoryKind::Architecture),
        "convention" => Some(MemoryKind::Convention),
        "workflow" => Some(MemoryKind::Workflow),
        "decision" => Some(MemoryKind::Decision),
        "deployment" => Some(MemoryKind::Deployment),
        "troubleshooting" => Some(MemoryKind::Troubleshooting),
        _ => None,
    }
}
