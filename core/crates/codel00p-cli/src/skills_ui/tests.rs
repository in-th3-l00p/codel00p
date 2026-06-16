use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::model::{Filter, Flow, Mutation, Screen, SkillKind, SkillRow, SkillsModel};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::empty())
}

fn row(name: &str, kind: SkillKind) -> SkillRow {
    SkillRow {
        name: name.to_string(),
        kind,
        source: "user".to_string(),
        created_by: Some("agent".to_string()),
        usage: 0,
        description: format!("{name} description"),
        body: "Do the thing.".to_string(),
        version: None,
        triggers: Vec::new(),
        path: format!("/skills/{name}/SKILL.md"),
        root: PathBuf::from("/skills"),
    }
}

fn model_with(rows: Vec<SkillRow>) -> SkillsModel {
    let mut model = SkillsModel::new();
    model.set_rows(rows);
    model
}

#[test]
fn tab_cycles_filter_without_a_store_reload() {
    let mut model = SkillsModel::new();
    assert_eq!(model.filter, Filter::Active);
    assert_eq!(model.update(key(KeyCode::Tab)), Flow::Stay);
    assert_eq!(model.filter, Filter::Candidates);
    assert_eq!(model.update(key(KeyCode::Tab)), Flow::Stay);
    assert_eq!(model.filter, Filter::All);
    assert_eq!(model.update(key(KeyCode::BackTab)), Flow::Stay);
    assert_eq!(model.filter, Filter::Candidates);
}

#[test]
fn filter_hides_rows_of_the_other_kind() {
    let mut model = model_with(vec![
        row("alpha", SkillKind::Active),
        row("beta", SkillKind::Candidate),
    ]);
    // Active view shows only the active skill.
    assert_eq!(model.picker.visible().count(), 1);
    assert_eq!(
        model.picker.selected_item().map(|r| r.name.as_str()),
        Some("alpha")
    );
    // Candidates view shows only the candidate.
    model.update(key(KeyCode::Tab));
    assert_eq!(
        model.picker.selected_item().map(|r| r.name.as_str()),
        Some("beta")
    );
}

#[test]
fn enter_opens_detail_for_selected_row() {
    let mut model = model_with(vec![row("alpha", SkillKind::Active)]);
    assert_eq!(model.update(key(KeyCode::Enter)), Flow::OpenDetail);
    assert_eq!(model.screen, Screen::Detail);
    assert_eq!(
        model.selected.as_ref().map(|r| r.name.as_str()),
        Some("alpha")
    );
}

#[test]
fn approve_from_list_targets_the_candidate() {
    let mut model = model_with(vec![row("beta", SkillKind::Candidate)]);
    model.update(key(KeyCode::Tab)); // switch to candidates view
    assert_eq!(
        model.update(key(KeyCode::Char('a'))),
        Flow::Mutate(Mutation::Approve {
            name: "beta".to_string(),
            root: PathBuf::from("/skills"),
        })
    );
}

#[test]
fn reject_from_detail_targets_the_candidate() {
    let mut model = model_with(vec![row("beta", SkillKind::Candidate)]);
    model.update(key(KeyCode::Tab));
    model.update(key(KeyCode::Enter));
    assert_eq!(model.screen, Screen::Detail);
    assert_eq!(
        model.update(key(KeyCode::Char('r'))),
        Flow::Mutate(Mutation::Reject {
            name: "beta".to_string(),
            root: PathBuf::from("/skills"),
        })
    );
}

#[test]
fn disable_targets_an_active_skill() {
    let mut model = model_with(vec![row("alpha", SkillKind::Active)]);
    assert_eq!(
        model.update(key(KeyCode::Char('d'))),
        Flow::Mutate(Mutation::Disable {
            name: "alpha".to_string(),
            root: PathBuf::from("/skills"),
        })
    );
}

#[test]
fn approve_on_an_active_skill_is_a_no_op_with_a_hint() {
    let mut model = model_with(vec![row("alpha", SkillKind::Active)]);
    assert_eq!(model.update(key(KeyCode::Char('a'))), Flow::Stay);
    assert!(model.status.is_some());
}

#[test]
fn disable_on_a_candidate_is_a_no_op_with_a_hint() {
    let mut model = model_with(vec![row("beta", SkillKind::Candidate)]);
    model.update(key(KeyCode::Tab));
    assert_eq!(model.update(key(KeyCode::Char('d'))), Flow::Stay);
    assert!(model.status.is_some());
}

#[test]
fn esc_from_detail_returns_to_list() {
    let mut model = model_with(vec![row("alpha", SkillKind::Active)]);
    model.update(key(KeyCode::Enter));
    assert_eq!(model.update(key(KeyCode::Esc)), Flow::Stay);
    assert_eq!(model.screen, Screen::List);
    assert!(model.selected.is_none());
}

#[test]
fn esc_from_list_quits() {
    let mut model = model_with(vec![row("alpha", SkillKind::Active)]);
    assert_eq!(model.update(key(KeyCode::Esc)), Flow::Quit);
}
