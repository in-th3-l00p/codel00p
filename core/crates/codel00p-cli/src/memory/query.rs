use codel00p_memory::{
    MemoryListFilter, MemoryQualityQuery, MemoryQuery, MemoryRepository, MemorySimilarityQuery,
    MemoryStalenessQuery,
};

use crate::config::{CliConfig, CliResult, open_memory_store, required_value};

use super::{
    json::{
        memory_record_json, quality_memory_json, retrieved_memory_json, similar_memory_json,
        source_uri, stale_memory_json,
    },
    parse::{kind_label, parse_kind, parse_sensitivity, parse_status, status_label},
};

pub(super) fn memory_quality(config: CliConfig, args: &[String]) -> CliResult<String> {
    let mut query = MemoryQualityQuery::new(config.project.clone());
    let mut json_output = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--status" => {
                query = query.with_status(parse_status(&required_value(args, index, "--status")?)?);
                index += 2;
            }
            "--kind" => {
                query = query.with_kind(parse_kind(&required_value(args, index, "--kind")?)?);
                index += 2;
            }
            "--sensitivity" => {
                query = query.with_sensitivity(parse_sensitivity(&required_value(
                    args,
                    index,
                    "--sensitivity",
                )?)?);
                index += 2;
            }
            "--tag" => {
                query = query.with_tag(required_value(args, index, "--tag")?);
                index += 2;
            }
            "--max-score" => {
                let score = required_value(args, index, "--max-score")?
                    .parse::<u8>()
                    .map_err(|_| "invalid --max-score".to_string())?;
                if score > 100 {
                    return Err("invalid --max-score".to_string());
                }
                query = query.with_max_score(score);
                index += 2;
            }
            "--limit" => {
                let limit = required_value(args, index, "--limit")?
                    .parse::<usize>()
                    .map_err(|_| "invalid --limit".to_string())?;
                query = query.with_limit(limit);
                index += 2;
            }
            "--json" => {
                json_output = true;
                index += 1;
            }
            flag => return Err(format!("unknown memory quality option: {flag}")),
        }
    }

    let store = open_memory_store(&config)?;
    let records = store
        .quality_review(query)
        .map_err(|error| error.to_string())?;
    if json_output {
        let items = records.iter().map(quality_memory_json).collect::<Vec<_>>();
        return serde_json::to_string(&items).map_err(|error| error.to_string());
    }

    let mut output = String::new();
    for memory in records {
        output.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\n",
            memory.entry().id(),
            status_label(memory.entry().status()),
            kind_label(memory.entry().kind()),
            memory.quality().score(),
            memory.entry().content()
        ));
    }
    Ok(output)
}

pub(super) fn memory_stale(config: CliConfig, args: &[String]) -> CliResult<String> {
    let mut query = MemoryStalenessQuery::new(config.project.clone());
    let mut json_output = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--kind" => {
                query = query.with_kind(parse_kind(&required_value(args, index, "--kind")?)?);
                index += 2;
            }
            "--threshold" => {
                let score = required_value(args, index, "--threshold")?
                    .parse::<u8>()
                    .map_err(|_| "invalid --threshold".to_string())?;
                if score > 100 {
                    return Err("invalid --threshold".to_string());
                }
                query = query.with_min_score(score);
                index += 2;
            }
            "--limit" => {
                let limit = required_value(args, index, "--limit")?
                    .parse::<usize>()
                    .map_err(|_| "invalid --limit".to_string())?;
                query = query.with_limit(limit);
                index += 2;
            }
            "--json" => {
                json_output = true;
                index += 1;
            }
            flag => return Err(format!("unknown memory stale option: {flag}")),
        }
    }

    let store = open_memory_store(&config)?;
    let records = store
        .stale_active(query)
        .map_err(|error| error.to_string())?;
    if json_output {
        let items = records.iter().map(stale_memory_json).collect::<Vec<_>>();
        return serde_json::to_string(&items).map_err(|error| error.to_string());
    }

    let mut output = String::new();
    for memory in records {
        output.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}\n",
            memory.entry().id(),
            status_label(memory.entry().status()),
            kind_label(memory.entry().kind()),
            memory.score(),
            memory.newer_entry().id(),
            memory.entry().content()
        ));
    }
    Ok(output)
}

pub(super) fn memory_similar(config: CliConfig, args: &[String]) -> CliResult<String> {
    let mut content = None;
    let mut kind = None;
    let mut threshold = None;
    let mut limit = None;
    let mut json_output = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--content" => {
                content = Some(required_value(args, index, "--content")?);
                index += 2;
            }
            "--kind" => {
                kind = Some(parse_kind(&required_value(args, index, "--kind")?)?);
                index += 2;
            }
            "--threshold" => {
                let score = required_value(args, index, "--threshold")?
                    .parse::<u8>()
                    .map_err(|_| "invalid --threshold".to_string())?;
                if score > 100 {
                    return Err("invalid --threshold".to_string());
                }
                threshold = Some(score);
                index += 2;
            }
            "--limit" => {
                limit = Some(
                    required_value(args, index, "--limit")?
                        .parse::<usize>()
                        .map_err(|_| "invalid --limit".to_string())?,
                );
                index += 2;
            }
            "--json" => {
                json_output = true;
                index += 1;
            }
            flag => return Err(format!("unknown memory similar option: {flag}")),
        }
    }

    let content = content.ok_or_else(|| "missing required --content".to_string())?;
    let kind = kind.ok_or_else(|| "missing required --kind".to_string())?;
    let mut query = MemorySimilarityQuery::new(config.project.clone(), kind, content);
    if let Some(threshold) = threshold {
        query = query.with_min_score(threshold);
    }
    if let Some(limit) = limit {
        query = query.with_limit(limit);
    }

    let store = open_memory_store(&config)?;
    let records = store
        .similar_active(query)
        .map_err(|error| error.to_string())?;
    if json_output {
        let items = records.iter().map(similar_memory_json).collect::<Vec<_>>();
        return serde_json::to_string(&items).map_err(|error| error.to_string());
    }

    let mut output = String::new();
    for memory in records {
        output.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\n",
            memory.entry().id(),
            status_label(memory.entry().status()),
            kind_label(memory.entry().kind()),
            memory.score(),
            memory.entry().content()
        ));
    }
    Ok(output)
}

pub(super) fn memory_search(config: CliConfig, args: &[String]) -> CliResult<String> {
    let mut query = MemoryQuery::new(config.project.clone());
    let mut json_output = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--text" => {
                query = query.with_text(required_value(args, index, "--text")?);
                index += 2;
            }
            "--kind" => {
                query = query.with_kind(parse_kind(&required_value(args, index, "--kind")?)?);
                index += 2;
            }
            "--tag" => {
                query = query.with_tag(required_value(args, index, "--tag")?);
                index += 2;
            }
            "--sensitivity" => {
                query = query.with_sensitivity(parse_sensitivity(&required_value(
                    args,
                    index,
                    "--sensitivity",
                )?)?);
                index += 2;
            }
            "--limit" => {
                let limit = required_value(args, index, "--limit")?
                    .parse::<usize>()
                    .map_err(|_| "invalid --limit".to_string())?;
                query = query.with_limit(limit);
                index += 2;
            }
            "--json" => {
                json_output = true;
                index += 1;
            }
            flag => return Err(format!("unknown memory search option: {flag}")),
        }
    }

    let store = open_memory_store(&config)?;
    let records = store.retrieve(query).map_err(|error| error.to_string())?;
    if json_output {
        let items = records
            .iter()
            .map(retrieved_memory_json)
            .collect::<Vec<_>>();
        return serde_json::to_string(&items).map_err(|error| error.to_string());
    }

    let mut output = String::new();
    for memory in records {
        output.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\n",
            memory.entry().id(),
            status_label(memory.entry().status()),
            kind_label(memory.entry().kind()),
            memory.reason(),
            memory.entry().content()
        ));
    }
    Ok(output)
}

pub(super) fn memory_list(config: CliConfig, args: &[String]) -> CliResult<String> {
    let mut filter = MemoryListFilter::new(config.project.clone());
    let mut json_output = false;
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
            "--sensitivity" => {
                filter = filter.with_sensitivity(parse_sensitivity(&required_value(
                    args,
                    index,
                    "--sensitivity",
                )?)?);
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
            "--json" => {
                json_output = true;
                index += 1;
            }
            flag => return Err(format!("unknown memory list option: {flag}")),
        }
    }

    let store = open_memory_store(&config)?;
    let records = store.list(filter).map_err(|error| error.to_string())?;
    if json_output {
        let items = records.iter().map(memory_record_json).collect::<Vec<_>>();
        return serde_json::to_string(&items).map_err(|error| error.to_string());
    }

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

pub(super) fn memory_show(config: CliConfig, args: &[String]) -> CliResult<String> {
    let Some(id) = args.first() else {
        return Err("memory show expects exactly one memory id".to_string());
    };
    let mut json_output = false;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                json_output = true;
                index += 1;
            }
            flag => return Err(format!("unknown memory show option: {flag}")),
        }
    }

    let store = open_memory_store(&config)?;
    let record = store.get(id).map_err(|error| error.to_string())?;
    if json_output {
        return serde_json::to_string(&memory_record_json(&record))
            .map_err(|error| error.to_string());
    }

    let mut output = format!(
        "id: {}\nstatus: {}\nkind: {}\ntags: {}\n",
        record.entry().id(),
        status_label(record.entry().status()),
        kind_label(record.entry().kind()),
        record.entry().tags().join(",")
    );
    if let Some(source) = record.entry().source() {
        output.push_str(&format!(
            "source_session: {}\nsource_turn: {}\nsource_uri: {}\n",
            source.session_id().as_str(),
            source.turn_id().as_str(),
            source_uri(source)
        ));
    }
    output.push_str(&format!("content: {}\n", record.entry().content()));

    Ok(output)
}
