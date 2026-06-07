use std::{env, path::PathBuf, process::ExitCode};

use codel00p_memory::{
    MemoryListFilter, MemoryRepository, ReviewDecision, StorageBackedMemoryStore,
};
use codel00p_protocol::{MemoryKind, MemoryStatus, ProjectRef};
use codel00p_storage::{SqliteStorage, StorageScope};

type CliResult<T> = Result<T, String>;

struct CliConfig {
    memory_db: PathBuf,
    organization_id: String,
    project: ProjectRef,
}

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(output) => {
            print!("{output}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> CliResult<String> {
    let (config, rest) = parse_global_args(args)?;
    let Some((command, rest)) = rest.split_first() else {
        return Err("missing command".to_string());
    };

    match command.as_str() {
        "memory" => run_memory(config, rest),
        _ => Err(format!("unknown command: {command}")),
    }
}

fn parse_global_args(args: Vec<String>) -> CliResult<(CliConfig, Vec<String>)> {
    let mut memory_db = None;
    let mut organization_id = None;
    let mut project_id = None;
    let mut project_name = None;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--memory-db" => {
                memory_db = Some(PathBuf::from(required_value(&args, index, "--memory-db")?));
                index += 2;
            }
            "--organization-id" => {
                organization_id = Some(required_value(&args, index, "--organization-id")?);
                index += 2;
            }
            "--project-id" => {
                project_id = Some(required_value(&args, index, "--project-id")?);
                index += 2;
            }
            "--project-name" => {
                project_name = Some(required_value(&args, index, "--project-name")?);
                index += 2;
            }
            _ => break,
        }
    }

    let config = CliConfig {
        memory_db: memory_db.ok_or_else(|| "missing required --memory-db".to_string())?,
        organization_id: organization_id
            .ok_or_else(|| "missing required --organization-id".to_string())?,
        project: ProjectRef::new(
            project_id.ok_or_else(|| "missing required --project-id".to_string())?,
            project_name.ok_or_else(|| "missing required --project-name".to_string())?,
        ),
    };

    Ok((config, args[index..].to_vec()))
}

fn required_value(args: &[String], index: usize, name: &str) -> CliResult<String> {
    args.get(index + 1)
        .cloned()
        .filter(|value| !value.starts_with("--"))
        .ok_or_else(|| format!("missing value for {name}"))
}

fn run_memory(config: CliConfig, args: &[String]) -> CliResult<String> {
    let Some((command, rest)) = args.split_first() else {
        return Err("missing memory command".to_string());
    };

    match command.as_str() {
        "list" => memory_list(config, rest),
        "show" => memory_show(config, rest),
        "audit" => memory_audit(config, rest),
        "approve" => memory_review(config, rest, ReviewCommand::Approve),
        "reject" => memory_review(config, rest, ReviewCommand::Reject),
        "archive" => memory_review(config, rest, ReviewCommand::Archive),
        _ => Err(format!("unknown memory command: {command}")),
    }
}

fn open_store(
    config: &CliConfig,
) -> CliResult<StorageBackedMemoryStore<codel00p_storage::SqliteStorage>> {
    let storage = SqliteStorage::open(&config.memory_db).map_err(|error| error.to_string())?;
    Ok(StorageBackedMemoryStore::new(
        StorageScope::project(&config.organization_id, config.project.id()),
        storage,
    ))
}

fn memory_list(config: CliConfig, args: &[String]) -> CliResult<String> {
    let mut filter = MemoryListFilter::new(config.project.clone());
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--status" => {
                filter =
                    filter.with_status(parse_status(&required_value(args, index, "--status")?)?);
                index += 2;
            }
            "--kind" => {
                filter = filter.with_kind(parse_kind(&required_value(args, index, "--kind")?)?);
                index += 2;
            }
            "--tag" => {
                filter = filter.with_tag(required_value(args, index, "--tag")?);
                index += 2;
            }
            "--limit" => {
                let limit = required_value(args, index, "--limit")?
                    .parse::<usize>()
                    .map_err(|_| "invalid --limit".to_string())?;
                filter = filter.with_limit(limit);
                index += 2;
            }
            flag => return Err(format!("unknown memory list option: {flag}")),
        }
    }

    let store = open_store(&config)?;
    let records = store.list(filter).map_err(|error| error.to_string())?;
    let mut output = String::new();
    for record in records {
        output.push_str(&format!(
            "{}\t{}\t{}\t{}\n",
            record.entry().id(),
            status_label(record.entry().status()),
            kind_label(record.entry().kind()),
            record.entry().content()
        ));
    }
    Ok(output)
}

fn memory_show(config: CliConfig, args: &[String]) -> CliResult<String> {
    let id = single_id(args, "memory show")?;
    let store = open_store(&config)?;
    let record = store.get(id).map_err(|error| error.to_string())?;

    Ok(format!(
        "id: {}\nstatus: {}\nkind: {}\ntags: {}\ncontent: {}\n",
        record.entry().id(),
        status_label(record.entry().status()),
        kind_label(record.entry().kind()),
        record.entry().tags().join(","),
        record.entry().content()
    ))
}

fn memory_audit(config: CliConfig, args: &[String]) -> CliResult<String> {
    let id = single_id(args, "memory audit")?;
    let store = open_store(&config)?;
    let audit = store.audit_log(id).map_err(|error| error.to_string())?;
    let mut output = String::new();
    for event in audit {
        output.push_str(&format!(
            "{}\t{}\t{}\t{}\n",
            event.sequence(),
            audit_action_label(event.action()),
            event.actor(),
            event.reason().unwrap_or("")
        ));
    }
    Ok(output)
}

enum ReviewCommand {
    Approve,
    Reject,
    Archive,
}

fn memory_review(config: CliConfig, args: &[String], command: ReviewCommand) -> CliResult<String> {
    let Some(id) = args.first() else {
        return Err("missing memory id".to_string());
    };
    let mut actor = None;
    let mut reason = None;
    let mut index = 1;

    while index < args.len() {
        match args[index].as_str() {
            "--actor" => {
                actor = Some(required_value(args, index, "--actor")?);
                index += 2;
            }
            "--reason" => {
                reason = Some(required_value(args, index, "--reason")?);
                index += 2;
            }
            flag => return Err(format!("unknown review option: {flag}")),
        }
    }

    let actor = actor.ok_or_else(|| "missing required --actor".to_string())?;
    let decision = match command {
        ReviewCommand::Approve => ReviewDecision::approve(actor),
        ReviewCommand::Reject => ReviewDecision::reject(
            actor,
            reason.ok_or_else(|| "missing required --reason".to_string())?,
        ),
        ReviewCommand::Archive => ReviewDecision::archive(
            actor,
            reason.ok_or_else(|| "missing required --reason".to_string())?,
        ),
    };

    let mut store = open_store(&config)?;
    let record = store
        .review(id, decision)
        .map_err(|error| error.to_string())?;

    Ok(format!(
        "{}\t{}\n",
        record.entry().id(),
        status_label(record.entry().status())
    ))
}

fn single_id<'a>(args: &'a [String], command: &str) -> CliResult<&'a str> {
    if args.len() != 1 {
        return Err(format!("{command} expects exactly one memory id"));
    }
    Ok(&args[0])
}

fn parse_status(value: &str) -> CliResult<MemoryStatus> {
    match value {
        "candidate" => Ok(MemoryStatus::Candidate),
        "approved" => Ok(MemoryStatus::Approved),
        "rejected" => Ok(MemoryStatus::Rejected),
        "archived" => Ok(MemoryStatus::Archived),
        _ => Err(format!("unknown memory status: {value}")),
    }
}

fn parse_kind(value: &str) -> CliResult<MemoryKind> {
    match value {
        "architecture" => Ok(MemoryKind::Architecture),
        "convention" => Ok(MemoryKind::Convention),
        "workflow" => Ok(MemoryKind::Workflow),
        "decision" => Ok(MemoryKind::Decision),
        "deployment" => Ok(MemoryKind::Deployment),
        "troubleshooting" => Ok(MemoryKind::Troubleshooting),
        _ => Err(format!("unknown memory kind: {value}")),
    }
}

fn status_label(status: MemoryStatus) -> &'static str {
    match status {
        MemoryStatus::Candidate => "candidate",
        MemoryStatus::Approved => "approved",
        MemoryStatus::Rejected => "rejected",
        MemoryStatus::Archived => "archived",
    }
}

fn kind_label(kind: MemoryKind) -> &'static str {
    match kind {
        MemoryKind::Architecture => "architecture",
        MemoryKind::Convention => "convention",
        MemoryKind::Workflow => "workflow",
        MemoryKind::Decision => "decision",
        MemoryKind::Deployment => "deployment",
        MemoryKind::Troubleshooting => "troubleshooting",
    }
}

fn audit_action_label(action: codel00p_memory::MemoryAuditAction) -> &'static str {
    match action {
        codel00p_memory::MemoryAuditAction::CandidateCreated => "candidate_created",
        codel00p_memory::MemoryAuditAction::Approved => "approved",
        codel00p_memory::MemoryAuditAction::Rejected => "rejected",
        codel00p_memory::MemoryAuditAction::Archived => "archived",
    }
}
