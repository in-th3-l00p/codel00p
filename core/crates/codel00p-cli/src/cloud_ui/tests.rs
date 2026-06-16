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
fn p_pushes_and_l_pulls_from_status() {
    let mut model = signed_in();
    assert_eq!(model.update(key(KeyCode::Char('p'))), Flow::Push);
    assert_eq!(model.update(key(KeyCode::Char('l'))), Flow::Pull);
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
