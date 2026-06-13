use super::*;
use serde_json::json;

#[test]
fn organization_scope_groups_documents_for_listing() {
    let mut storage = InMemoryStorage::default();
    let acme = StorageScope::organization("org_acme");
    let other = StorageScope::organization("org_other");

    assert_eq!(acme.organization_id(), Some("org_acme"));
    assert_eq!(acme.project_id(), None);

    storage
        .put_document(StorageDocument::new(
            acme.clone(),
            "projects",
            "proj_1",
            json!({ "name": "alpha" }),
        ))
        .expect("put alpha");
    storage
        .put_document(StorageDocument::new(
            acme.clone(),
            "projects",
            "proj_2",
            json!({ "name": "beta" }),
        ))
        .expect("put beta");
    storage
        .put_document(StorageDocument::new(
            other,
            "projects",
            "proj_3",
            json!({ "name": "gamma" }),
        ))
        .expect("put gamma");

    let listed = storage
        .list_documents(&acme, "projects")
        .expect("list org projects");

    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].id(), "proj_1");
    assert_eq!(listed[1].id(), "proj_2");
}

#[test]
fn documents_can_be_deleted() {
    let mut storage = InMemoryStorage::default();
    let scope = StorageScope::project("org_acme", "proj_1");
    storage
        .put_document(StorageDocument::new(
            scope.clone(),
            "agents",
            "agent_1",
            json!({ "name": "reviewer" }),
        ))
        .expect("put");

    assert!(
        storage
            .delete_document(&scope, "agents", "agent_1")
            .expect("delete")
    );
    assert!(
        !storage
            .delete_document(&scope, "agents", "agent_1")
            .expect("delete missing")
    );
    assert!(
        storage
            .get_document(&scope, "agents", "agent_1")
            .expect("get")
            .is_none()
    );
}
