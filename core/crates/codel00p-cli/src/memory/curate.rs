use codel00p_memory::{
    Consolidation, DEFAULT_CONSOLIDATION_THRESHOLD, MemoryListFilter, MemoryRepository,
    ReviewDecision, plan_consolidations,
};
use codel00p_protocol::MemoryStatus;

use crate::config::{CliConfig, CliResult, open_memory_store, required_value};

use super::parse::kind_label;

/// `codel00p memory curate` — the per-agent curator pass over near-duplicate
/// memories. Detection reuses the offline shingle similarity; the default is a
/// dry-run report. `--apply` archives (never deletes) each cluster's duplicates
/// in favor of the surviving memory, through the existing review/archive path so
/// the action is audited and reversible.
pub(super) fn memory_curate(config: CliConfig, args: &[String]) -> CliResult<String> {
    let mut threshold = DEFAULT_CONSOLIDATION_THRESHOLD;
    let mut apply = false;
    let mut actor = None;
    let mut json_output = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--threshold" => {
                let score = required_value(args, index, "--threshold")?
                    .parse::<u8>()
                    .map_err(|_| "invalid --threshold".to_string())?;
                if score > 100 {
                    return Err("invalid --threshold".to_string());
                }
                threshold = score;
                index += 2;
            }
            "--actor" => {
                actor = Some(required_value(args, index, "--actor")?);
                index += 2;
            }
            "--apply" => {
                apply = true;
                index += 1;
            }
            "--json" => {
                json_output = true;
                index += 1;
            }
            flag => return Err(format!("unknown memory curate option: {flag}")),
        }
    }

    let mut store = open_memory_store(&config)?;
    // Curate the durable knowledge layer: only approved active memories.
    let records = store
        .list(MemoryListFilter::new(config.project.clone()).with_status(MemoryStatus::Approved))
        .map_err(|error| error.to_string())?;
    let plan = plan_consolidations(&records, threshold);

    if json_output {
        return Ok(serde_json::to_string(&plan_json(&plan, apply)).unwrap_or_else(|_| "[]".into()));
    }

    if plan.is_empty() {
        return Ok("No near-duplicate memories to consolidate.\n".to_string());
    }

    if !apply {
        return Ok(render_dry_run(&plan, threshold));
    }

    let actor = actor.unwrap_or_else(crate::actor::infer_actor);
    let mut archived = 0_usize;
    for consolidation in &plan {
        let survivor_id = consolidation.survivor().entry().id().to_string();
        for duplicate in consolidation.duplicates() {
            let reason = format!(
                "curator: near-duplicate of {survivor_id} ({}% similar)",
                duplicate.similarity()
            );
            store
                .review(
                    duplicate.entry().id(),
                    ReviewDecision::archive(actor.clone(), reason),
                )
                .map_err(|error| error.to_string())?;
            archived += 1;
        }
    }

    Ok(format!(
        "Archived {archived} duplicate memory record(s) across {} cluster(s); survivors kept active.\n",
        plan.len()
    ))
}

fn render_dry_run(plan: &[Consolidation], threshold: u8) -> String {
    let mut output = String::new();
    let mut total_duplicates = 0_usize;
    for consolidation in plan {
        let survivor = consolidation.survivor();
        output.push_str(&format!(
            "keep\t{}\t{}\tq{}\t{}\n",
            survivor.entry().id(),
            kind_label(survivor.entry().kind()),
            survivor.quality().score(),
            survivor.entry().content(),
        ));
        for duplicate in consolidation.duplicates() {
            total_duplicates += 1;
            output.push_str(&format!(
                "archive\t{}\t{}\t{}%\t{}\n",
                duplicate.entry().id(),
                kind_label(duplicate.entry().kind()),
                duplicate.similarity(),
                duplicate.entry().content(),
            ));
        }
        output.push('\n');
    }
    output.push_str(&format!(
        "{} cluster(s), {total_duplicates} duplicate(s) at \u{2265}{threshold}% similarity. \
Re-run with --apply to archive duplicates (reversible).\n",
        plan.len()
    ));
    output
}

fn plan_json(plan: &[Consolidation], applied: bool) -> serde_json::Value {
    let clusters: Vec<serde_json::Value> = plan
        .iter()
        .map(|consolidation| {
            let survivor = consolidation.survivor();
            serde_json::json!({
                "survivor": {
                    "id": survivor.entry().id(),
                    "kind": kind_label(survivor.entry().kind()),
                    "quality": survivor.quality().score(),
                    "content": survivor.entry().content(),
                },
                "duplicates": consolidation
                    .duplicates()
                    .iter()
                    .map(|duplicate| serde_json::json!({
                        "id": duplicate.entry().id(),
                        "kind": kind_label(duplicate.entry().kind()),
                        "similarity": duplicate.similarity(),
                        "content": duplicate.entry().content(),
                    }))
                    .collect::<Vec<_>>(),
            })
        })
        .collect();
    serde_json::json!({ "applied": applied, "clusters": clusters })
}
