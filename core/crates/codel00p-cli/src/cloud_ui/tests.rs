use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::model::{CloudModel, DetailTab, EntityRow, Flow, ProjectRow, Screen};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::empty())
}

fn project(id: &str, name: &str) -> ProjectRow {
    ProjectRow {
        id: id.to_string(),
        name: name.to_string(),
        detail: Some("git@example".to_string()),
    }
}

fn entity(label: &str) -> EntityRow {
    EntityRow {
        label: label.to_string(),
        detail: Some("detail".to_string()),
    }
}

fn signed_in() -> CloudModel {
    CloudModel::signed_in(
        vec!["user: u1".to_string(), "org: Acme (org_1)".to_string()],
        vec![project("proj_1", "alpha"), project("proj_2", "beta")],
    )
}

#[test]
fn enter_opens_selected_project() {
    let mut model = signed_in();
    assert_eq!(
        model.update(key(KeyCode::Enter)),
        Flow::OpenProject("proj_1".to_string())
    );
}

#[test]
fn esc_on_status_quits() {
    let mut model = signed_in();
    assert_eq!(model.update(key(KeyCode::Esc)), Flow::Quit);
}

#[test]
fn push_pull_without_active_project_prompt_to_select() {
    // With no project opened yet, push/pull stay (no Flow) and set a clear,
    // non-error status telling the user to select a project first.
    let mut model = signed_in();
    assert_eq!(model.update(key(KeyCode::Char('p'))), Flow::Stay);
    assert!(
        model
            .status
            .as_deref()
            .is_some_and(|s| s.contains("Select a project"))
    );

    model.status = None;
    assert_eq!(model.update(key(KeyCode::Char('l'))), Flow::Stay);
    assert!(
        model
            .status
            .as_deref()
            .is_some_and(|s| s.contains("Select a project"))
    );
}

#[test]
fn seeded_active_project_targets_push_pull_without_opening() {
    // A preconfigured active project (e.g. from CODEL00P_CLOUD_PROJECT) lets
    // push/pull target it before the user opens anything.
    let mut model = signed_in();
    model.set_active_project(project("proj_2", "beta"));
    assert_eq!(model.active_project_id(), Some("proj_2"));
    assert_eq!(
        model.update(key(KeyCode::Char('p'))),
        Flow::Push("proj_2".to_string())
    );
}

#[test]
fn opening_a_project_targets_push_pull_at_its_id() {
    // Opening a project sets it as the active push/pull target, even after
    // returning to the status screen.
    let mut model = signed_in();
    model.show_detail(
        project("proj_2", "beta"),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    );
    assert_eq!(model.active_project_id(), Some("proj_2"));

    model.update(key(KeyCode::Esc));
    assert_eq!(model.screen, Screen::Status);
    assert_eq!(
        model.update(key(KeyCode::Char('p'))),
        Flow::Push("proj_2".to_string())
    );
    assert_eq!(
        model.update(key(KeyCode::Char('l'))),
        Flow::Pull("proj_2".to_string())
    );
}

#[test]
fn ctrl_c_quits_anywhere() {
    let mut model = signed_in();
    let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
    assert_eq!(model.update(ctrl_c), Flow::Quit);
}

#[test]
fn show_detail_switches_screen_and_tabs_cycle() {
    let mut model = signed_in();
    model.show_detail(
        project("proj_1", "alpha"),
        vec![entity("Reviewer")],
        vec![entity("github")],
        vec![entity("Run cargo from core/.")],
    );
    assert_eq!(model.screen, Screen::Detail);
    assert_eq!(model.tab, DetailTab::Agents);

    assert_eq!(model.update(key(KeyCode::Tab)), Flow::Stay);
    assert_eq!(model.tab, DetailTab::Mcp);
    assert_eq!(model.update(key(KeyCode::Right)), Flow::Stay);
    assert_eq!(model.tab, DetailTab::Memory);
    assert_eq!(model.update(key(KeyCode::Right)), Flow::Stay);
    assert_eq!(model.tab, DetailTab::Agents);
    assert_eq!(model.update(key(KeyCode::BackTab)), Flow::Stay);
    assert_eq!(model.tab, DetailTab::Memory);
}

#[test]
fn esc_on_detail_returns_to_status() {
    let mut model = signed_in();
    model.show_detail(
        project("proj_1", "alpha"),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    );
    assert_eq!(model.update(key(KeyCode::Esc)), Flow::Stay);
    assert_eq!(model.screen, Screen::Status);
}

#[test]
fn detail_navigation_does_not_leak_push_pull() {
    // On the detail screen, `p`/`l` filter the picker rather than triggering actions.
    let mut model = signed_in();
    model.show_detail(
        project("proj_1", "alpha"),
        vec![entity("planner"), entity("reviewer")],
        Vec::new(),
        Vec::new(),
    );
    assert_eq!(model.update(key(KeyCode::Char('p'))), Flow::Stay);
    assert_eq!(model.active_tab_picker().query(), "p");
}

#[test]
fn unauthenticated_quits_on_any_exit_key() {
    let mut model = CloudModel::unauthenticated("nope".to_string());
    assert_eq!(model.screen, Screen::Unauthenticated);
    assert_eq!(model.update(key(KeyCode::Enter)), Flow::Quit);

    let mut model = CloudModel::unauthenticated("nope".to_string());
    assert_eq!(model.update(key(KeyCode::Esc)), Flow::Quit);
}

#[test]
fn set_status_records_action_result() {
    let mut model = signed_in();
    model.set_status("Pushed 3 memories.");
    assert_eq!(model.status.as_deref(), Some("Pushed 3 memories."));
}

#[test]
fn question_mark_toggles_help_and_any_key_closes_it() {
    let mut model = signed_in();
    assert!(!model.show_help);

    // `?` opens the overlay.
    assert_eq!(model.update(key(KeyCode::Char('?'))), Flow::Stay);
    assert!(model.show_help);

    // While shown, any key (here Esc) closes it without quitting or acting.
    assert_eq!(model.update(key(KeyCode::Esc)), Flow::Stay);
    assert!(!model.show_help);
    assert_eq!(model.screen, Screen::Status);
}

#[test]
fn help_swallows_action_keys_while_open() {
    // A key that would normally push (p) just closes the help overlay instead.
    let mut model = signed_in();
    model.show_help = true;
    assert_eq!(model.update(key(KeyCode::Char('p'))), Flow::Stay);
    assert!(!model.show_help);
    assert!(model.status.is_none());
}

#[test]
fn ctrl_c_still_quits_even_with_help_open() {
    let mut model = signed_in();
    model.show_help = true;
    let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
    assert_eq!(model.update(ctrl_c), Flow::Quit);
}
