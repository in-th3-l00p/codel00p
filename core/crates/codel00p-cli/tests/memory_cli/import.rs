use super::support::*;

#[test]
fn memory_import_creates_single_candidate_from_whole_file() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");

    let file = dir.path().join("knowledge.md");
    std::fs::write(&file, "Use pnpm for all package management.\n").expect("write file");

    let output = run_codel00p(&db_path, &["memory", "import", file.to_str().unwrap()]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("imported 1 candidate(s)"), "output: {out}");
    assert!(out.contains("created\t"), "output: {out}");

    // Verify in store
    let storage = SqliteStorage::open(&db_path).expect("open sqlite");
    let store = StorageBackedMemoryStore::new(StorageScope::project("org-1", "project-1"), storage);
    let listed = store
        .list(MemoryListFilter::new(project()).with_status(MemoryStatus::Candidate))
        .expect("list");
    assert_eq!(listed.len(), 1);
    let entry = listed[0].entry();
    assert_eq!(entry.content(), "Use pnpm for all package management.");
    // source URI is the absolute file path
    let source = entry.source().expect("source set");
    assert!(source.is_import(), "should be import source");
    let abs = file.canonicalize().unwrap();
    assert_eq!(source.uri(), Some(abs.to_str().unwrap()));
}

#[test]
fn memory_import_respects_kind_and_tag() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");

    let file = dir.path().join("arch.md");
    std::fs::write(&file, "Service mesh routes all internal traffic.").expect("write file");

    let output = run_codel00p(
        &db_path,
        &[
            "memory",
            "import",
            file.to_str().unwrap(),
            "--kind",
            "architecture",
            "--tag",
            "infra",
        ],
    );
    assert!(output.status.success(), "stderr: {}", stderr(&output));

    let storage = SqliteStorage::open(&db_path).expect("open sqlite");
    let store = StorageBackedMemoryStore::new(StorageScope::project("org-1", "project-1"), storage);
    let listed = store
        .list(MemoryListFilter::new(project()).with_status(MemoryStatus::Candidate))
        .expect("list");
    assert_eq!(listed.len(), 1);
    let entry = listed[0].entry();
    assert_eq!(entry.kind(), MemoryKind::Architecture);
    assert!(
        entry.tags().contains(&"infra".to_string()),
        "tags: {:?}",
        entry.tags()
    );
}

#[test]
fn memory_import_split_sections_creates_one_candidate_per_heading() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");

    let content = "# Architecture\nUse hexagonal architecture.\n\n# Workflow\nRun pnpm verify before push.\n\n# Deployment\nDeploy via Vercel.\n";
    let file = dir.path().join("sections.md");
    std::fs::write(&file, content).expect("write file");

    let output = run_codel00p(
        &db_path,
        &[
            "memory",
            "import",
            file.to_str().unwrap(),
            "--split-sections",
        ],
    );
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("imported 3 candidate(s)"), "output: {out}");

    let storage = SqliteStorage::open(&db_path).expect("open sqlite");
    let store = StorageBackedMemoryStore::new(StorageScope::project("org-1", "project-1"), storage);
    let listed = store
        .list(MemoryListFilter::new(project()).with_status(MemoryStatus::Candidate))
        .expect("list");
    assert_eq!(listed.len(), 3);

    let contents: Vec<_> = listed.iter().map(|r| r.entry().content()).collect();
    assert!(
        contents.iter().any(|c| c.contains("hexagonal")),
        "contents: {contents:?}"
    );
    assert!(
        contents.iter().any(|c| c.contains("pnpm")),
        "contents: {contents:?}"
    );
    assert!(
        contents.iter().any(|c| c.contains("Vercel")),
        "contents: {contents:?}"
    );
}

#[test]
fn memory_import_skips_duplicate_id_with_warning() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");

    let file = dir.path().join("dup.md");
    std::fs::write(&file, "Convention: prefer composition over inheritance.").expect("write");

    // First import
    let first = run_codel00p(&db_path, &["memory", "import", file.to_str().unwrap()]);
    assert!(
        first.status.success(),
        "first import stderr: {}",
        stderr(&first)
    );
    let out1 = stdout(&first);
    assert!(out1.contains("imported 1 candidate(s)"), "first: {out1}");

    // Second import — same file, same derived id — should skip, not error
    let second = run_codel00p(&db_path, &["memory", "import", file.to_str().unwrap()]);
    assert!(
        second.status.success(),
        "second import stderr: {}",
        stderr(&second)
    );
    let out2 = stdout(&second);
    assert!(out2.contains("imported 0 candidate(s)"), "second: {out2}");
    assert!(out2.contains("skipped"), "second: {out2}");

    // Only one candidate in store
    let storage = SqliteStorage::open(&db_path).expect("open sqlite");
    let store = StorageBackedMemoryStore::new(StorageScope::project("org-1", "project-1"), storage);
    let listed = store
        .list(MemoryListFilter::new(project()).with_status(MemoryStatus::Candidate))
        .expect("list");
    assert_eq!(listed.len(), 1);
}

#[test]
fn memory_import_json_output() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");

    let file = dir.path().join("ref.md");
    std::fs::write(&file, "Use Rust for all core services.").expect("write");

    let output = run_codel00p(
        &db_path,
        &["memory", "import", file.to_str().unwrap(), "--json"],
    );
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let json: serde_json::Value = serde_json::from_str(&stdout(&output)).expect("valid json");
    let arr = json.as_array().expect("array");
    assert_eq!(arr.len(), 1);
    let item = &arr[0];
    assert_eq!(item["status"], "candidate");
    assert!(
        item["id"].as_str().unwrap().starts_with("import-"),
        "id: {}",
        item["id"]
    );
    // source_uri should be the file path
    let abs = file.canonicalize().unwrap();
    assert_eq!(item["source_uri"], abs.to_str().unwrap());
}

#[test]
fn memory_import_split_sections_skips_empty_sections() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("memory.sqlite");

    // Only the first section has content; second has just whitespace
    let content = "# Architecture\nUse ports and adapters.\n\n#   \n\n";
    let file = dir.path().join("sparse.md");
    std::fs::write(&file, content).expect("write");

    // The second line "# " with spaces doesn't start with "# " (needs exactly "# ")
    // so it is not treated as a heading at all. But let's also test a real empty section:
    let content2 =
        "# Heading One\nSome content here.\n\n# Heading Two\n\n# Heading Three\nMore content.\n";
    let file2 = dir.path().join("sparse2.md");
    std::fs::write(&file2, content2).expect("write");

    let output = run_codel00p(
        &db_path,
        &[
            "memory",
            "import",
            file2.to_str().unwrap(),
            "--split-sections",
        ],
    );
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    // Heading Two is empty → only 2 candidates
    assert!(out.contains("imported 2 candidate(s)"), "output: {out}");
}
