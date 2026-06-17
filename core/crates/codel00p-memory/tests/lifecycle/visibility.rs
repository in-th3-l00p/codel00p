use super::support::*;

use codel00p_protocol::MemoryVisibility;

fn seed_approved(
    store: &mut InMemoryMemoryStore,
    id: &str,
    content: &str,
    visibility: MemoryVisibility,
) {
    store
        .create_candidate(
            MemoryCandidateInput::new(id, project(), MemoryKind::Workflow, content, source())
                .with_tag("scoped")
                .with_visibility(visibility),
        )
        .expect("create candidate");
    store
        .review(id, ReviewDecision::approve("alice"))
        .expect("approve candidate");
}

#[test]
fn candidate_input_defaults_to_project_visibility() {
    let input = MemoryCandidateInput::new(
        "mem-default",
        project(),
        MemoryKind::Workflow,
        "Default scoped memory.",
        source(),
    );
    assert_eq!(input.visibility(), MemoryVisibility::Project);
}

#[test]
fn visibility_is_persisted_on_the_stored_entry() {
    let mut store = InMemoryMemoryStore::default();
    seed_approved(
        &mut store,
        "mem-private",
        "Private note.",
        MemoryVisibility::Private,
    );

    let record = store.get("mem-private").expect("load memory");
    assert_eq!(record.entry().visibility(), MemoryVisibility::Private);
}

#[test]
fn retrieve_without_visibility_filter_returns_all_visibilities() {
    let mut store = InMemoryMemoryStore::default();
    seed_approved(
        &mut store,
        "mem-private",
        "Private note.",
        MemoryVisibility::Private,
    );
    seed_approved(
        &mut store,
        "mem-project",
        "Project note.",
        MemoryVisibility::Project,
    );
    seed_approved(&mut store, "mem-org", "Org note.", MemoryVisibility::Org);

    let retrieved = store
        .retrieve(MemoryQuery::new(project()).with_tag("scoped"))
        .expect("retrieve memory");

    assert_eq!(
        retrieved
            .iter()
            .map(|memory| memory.entry().id())
            .collect::<Vec<_>>(),
        ["mem-org", "mem-private", "mem-project"]
    );
}

#[test]
fn retrieve_with_visibility_filter_returns_only_in_scope_memories() {
    let mut store = InMemoryMemoryStore::default();
    seed_approved(
        &mut store,
        "mem-private",
        "Private note.",
        MemoryVisibility::Private,
    );
    seed_approved(
        &mut store,
        "mem-project",
        "Project note.",
        MemoryVisibility::Project,
    );
    seed_approved(&mut store, "mem-team", "Team note.", MemoryVisibility::Team);
    seed_approved(&mut store, "mem-org", "Org note.", MemoryVisibility::Org);

    let retrieved = store
        .retrieve(
            MemoryQuery::new(project())
                .with_tag("scoped")
                .with_visibility(MemoryVisibility::Project),
        )
        .expect("retrieve memory");

    // A max-visibility of Project returns Private and Project, not Team or Org.
    assert_eq!(
        retrieved
            .iter()
            .map(|memory| memory.entry().id())
            .collect::<Vec<_>>(),
        ["mem-private", "mem-project"]
    );
    assert_eq!(
        retrieved
            .iter()
            .find(|memory| memory.entry().id() == "mem-project")
            .expect("project memory present")
            .reason(),
        "matched tag scoped and visibility project"
    );
}

#[test]
fn ranked_retrieval_respects_visibility_scope() {
    let mut store = InMemoryMemoryStore::default();
    seed_approved(
        &mut store,
        "mem-private",
        "Run pnpm verify before pushing main.",
        MemoryVisibility::Private,
    );
    seed_approved(
        &mut store,
        "mem-org",
        "Run pnpm verify inside the org dashboard.",
        MemoryVisibility::Org,
    );

    let ranked = store
        .retrieve_ranked(
            MemoryRetrievalQuery::new(project(), "pnpm verify")
                .with_visibility(MemoryVisibility::Private),
        )
        .expect("ranked retrieval");

    assert_eq!(ranked.len(), 1);
    assert_eq!(ranked[0].entry().id(), "mem-private");
}

#[test]
fn list_filters_by_max_visibility_scope() {
    let mut store = InMemoryMemoryStore::default();
    seed_approved(
        &mut store,
        "mem-private",
        "Private note.",
        MemoryVisibility::Private,
    );
    seed_approved(&mut store, "mem-team", "Team note.", MemoryVisibility::Team);
    seed_approved(&mut store, "mem-org", "Org note.", MemoryVisibility::Org);

    let listed = store
        .list(MemoryListFilter::new(project()).with_visibility(MemoryVisibility::Team))
        .expect("list memory");

    assert_eq!(
        listed
            .iter()
            .map(|record| record.entry().id())
            .collect::<Vec<_>>(),
        ["mem-private", "mem-team"]
    );
}
