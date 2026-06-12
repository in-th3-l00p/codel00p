use std::sync::atomic::{AtomicU64, Ordering};

use codel00p_protocol::{NewProject, Project, ProjectUpdate};
use codel00p_storage::{DocumentStore, StorageDocument, StorageScope};

use crate::error::ApiError;

const PROJECTS_COLLECTION: &str = "projects";
static PROJECT_COUNTER: AtomicU64 = AtomicU64::new(1);

fn store_project<S: DocumentStore + ?Sized>(
    store: &mut S,
    org_id: &str,
    project: &Project,
) -> Result<(), ApiError> {
    let scope = StorageScope::organization(org_id);
    let payload =
        serde_json::to_value(project).map_err(|err| ApiError::Internal(err.to_string()))?;
    let document = StorageDocument::new(scope, PROJECTS_COLLECTION, project.id(), payload);
    store
        .put_document(document)
        .map_err(|err| ApiError::Internal(err.to_string()))?;
    Ok(())
}

/// Applies a partial update to a project (slug stays stable). Admin-gated.
pub fn update_project<S: DocumentStore + ?Sized>(
    store: &mut S,
    org_id: &str,
    project_id: &str,
    update: ProjectUpdate,
) -> Result<Project, ApiError> {
    let existing = get_project(store, org_id, project_id)?
        .ok_or_else(|| ApiError::NotFound(format!("project {project_id} not found")))?;

    let name = update
        .name
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| existing.name().to_string());
    let mut project = Project::new(existing.id(), org_id, name, existing.slug());
    let repository_url = update
        .repository_url
        .or_else(|| existing.repository_url().map(str::to_string));
    if let Some(url) = repository_url {
        project = project.with_repository_url(url);
    }

    store_project(store, org_id, &project)?;
    Ok(project)
}

/// Deletes a project. Admin-gated. Returns NotFound if it does not exist.
pub fn delete_project<S: DocumentStore + ?Sized>(
    store: &mut S,
    org_id: &str,
    project_id: &str,
) -> Result<(), ApiError> {
    let scope = StorageScope::organization(org_id);
    let deleted = store
        .delete_document(&scope, PROJECTS_COLLECTION, project_id)
        .map_err(|err| ApiError::Internal(err.to_string()))?;
    if deleted {
        Ok(())
    } else {
        Err(ApiError::NotFound(format!(
            "project {project_id} not found"
        )))
    }
}

/// Lists the projects owned by an organization, ordered by id.
pub fn list_projects<S: DocumentStore + ?Sized>(
    store: &S,
    org_id: &str,
) -> Result<Vec<Project>, ApiError> {
    let scope = StorageScope::organization(org_id);
    let documents = store
        .list_documents(&scope, PROJECTS_COLLECTION)
        .map_err(|err| ApiError::Internal(err.to_string()))?;

    documents
        .into_iter()
        .map(|document| {
            serde_json::from_value(document.payload().clone())
                .map_err(|err| ApiError::Internal(format!("corrupt project record: {err}")))
        })
        .collect()
}

/// Fetches a single project owned by `org_id`, if it exists.
pub fn get_project<S: DocumentStore + ?Sized>(
    store: &S,
    org_id: &str,
    project_id: &str,
) -> Result<Option<Project>, ApiError> {
    let scope = StorageScope::organization(org_id);
    let document = store
        .get_document(&scope, PROJECTS_COLLECTION, project_id)
        .map_err(|err| ApiError::Internal(err.to_string()))?;

    document
        .map(|document| {
            serde_json::from_value(document.payload().clone())
                .map_err(|err| ApiError::Internal(format!("corrupt project record: {err}")))
        })
        .transpose()
}

/// Creates a project owned by `org_id` from a validated request body.
pub fn create_project<S: DocumentStore + ?Sized>(
    store: &mut S,
    org_id: &str,
    request: NewProject,
) -> Result<Project, ApiError> {
    let name = request.name.trim();
    if name.is_empty() {
        return Err(ApiError::BadRequest("project name is required".into()));
    }

    let id = format!("proj_{}", PROJECT_COUNTER.fetch_add(1, Ordering::Relaxed));
    let slug = slugify(name);
    let mut project = Project::new(id, org_id, name, slug);
    if let Some(url) = request.repository_url {
        project = project.with_repository_url(url);
    }

    let scope = StorageScope::organization(org_id);
    let payload =
        serde_json::to_value(&project).map_err(|err| ApiError::Internal(err.to_string()))?;
    let document = StorageDocument::new(scope, PROJECTS_COLLECTION, project.id(), payload);
    store
        .put_document(document)
        .map_err(|err| ApiError::Internal(err.to_string()))?;

    Ok(project)
}

/// Lowercases, keeps alphanumerics, and collapses every other run into a single
/// hyphen — a stable, URL-safe slug derived from a project name.
fn slugify(name: &str) -> String {
    let mut slug = String::new();
    let mut pending_separator = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            if pending_separator && !slug.is_empty() {
                slug.push('-');
            }
            pending_separator = false;
            slug.push(ch.to_ascii_lowercase());
        } else {
            pending_separator = true;
        }
    }
    if slug.is_empty() {
        slug.push_str("project");
    }
    slug
}

#[cfg(test)]
mod tests {
    use super::*;
    use codel00p_storage::InMemoryStorage;

    #[test]
    fn slugify_normalizes_names() {
        assert_eq!(slugify("codel00p Core"), "codel00p-core");
        assert_eq!(slugify("  Spaced  Out  "), "spaced-out");
        assert_eq!(slugify("!!!"), "project");
    }

    #[test]
    fn create_then_list_round_trips_within_org() {
        let mut store = InMemoryStorage::default();

        let created = create_project(
            &mut store,
            "org_acme",
            NewProject {
                name: "Alpha".into(),
                repository_url: Some("https://example.com/alpha".into()),
            },
        )
        .expect("create alpha");
        create_project(&mut store, "org_acme", NewProject::new("Beta")).expect("create beta");
        create_project(&mut store, "org_other", NewProject::new("Gamma")).expect("create gamma");

        assert_eq!(created.org_id(), "org_acme");
        assert_eq!(created.slug(), "alpha");
        assert_eq!(created.repository_url(), Some("https://example.com/alpha"));

        let acme = list_projects(&store, "org_acme").expect("list acme");
        let names: Vec<&str> = acme.iter().map(Project::name).collect();
        assert_eq!(names, vec!["Alpha", "Beta"]);

        let other = list_projects(&store, "org_other").expect("list other");
        assert_eq!(other.len(), 1);
        assert_eq!(other[0].name(), "Gamma");
    }

    #[test]
    fn create_rejects_blank_name() {
        let mut store = InMemoryStorage::default();
        let error = create_project(&mut store, "org_acme", NewProject::new("   ")).unwrap_err();
        assert!(matches!(error, ApiError::BadRequest(_)));
    }
}
