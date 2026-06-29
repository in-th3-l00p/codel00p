use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::model::{
    Filter, Flow, Mutation, NearDuplicate, Screen, SkillKind, SkillRow, SkillsModel,
};

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
        near_duplicate_of: None,
    }
}

/// An active skill the curator flagged as a near-duplicate of `survivor`.
fn dup_row(name: &str, survivor: &str, similarity: u8) -> SkillRow {
    SkillRow {
        near_duplicate_of: Some(NearDuplicate {
            survivor: survivor.to_string(),
            similarity,
        }),
        ..row(name, SkillKind::Active)
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
    assert_eq!(model.filter, Filter::Disabled);
    assert_eq!(model.update(key(KeyCode::Tab)), Flow::Stay);
    assert_eq!(model.filter, Filter::All);
    assert_eq!(model.update(key(KeyCode::BackTab)), Flow::Stay);
    assert_eq!(model.filter, Filter::Disabled);
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
fn disable_asks_to_confirm_then_applies_on_y() {
    let mut model = model_with(vec![row("alpha", SkillKind::Active)]);
    // First `d` arms the confirmation; it does NOT mutate yet.
    assert_eq!(model.update(key(KeyCode::Char('d'))), Flow::Stay);
    assert!(model.pending_disable.is_some());
    assert!(
        model
            .status
            .as_deref()
            .unwrap_or_default()
            .contains("confirm disabling")
    );
    // `y` confirms and emits the disable mutation.
    assert_eq!(
        model.update(key(KeyCode::Char('y'))),
        Flow::Mutate(Mutation::Disable {
            name: "alpha".to_string(),
            root: PathBuf::from("/skills"),
        })
    );
    assert!(model.pending_disable.is_none());
}

#[test]
fn disable_confirmation_cancels_on_any_other_key() {
    let mut model = model_with(vec![row("alpha", SkillKind::Active)]);
    model.update(key(KeyCode::Char('d')));
    assert!(model.pending_disable.is_some());
    // Any non-`y` key cancels without mutating.
    assert_eq!(model.update(key(KeyCode::Char('n'))), Flow::Stay);
    assert!(model.pending_disable.is_none());
    assert!(
        model
            .status
            .as_deref()
            .unwrap_or_default()
            .contains("Cancelled")
    );
}

#[test]
fn restore_targets_a_disabled_skill() {
    let mut model = model_with(vec![row("gamma", SkillKind::Disabled)]);
    model.update(key(KeyCode::Tab)); // Active -> Candidates
    model.update(key(KeyCode::Tab)); // Candidates -> Disabled
    assert_eq!(model.filter, Filter::Disabled);
    assert_eq!(
        model.update(key(KeyCode::Char('u'))),
        Flow::Mutate(Mutation::Restore {
            name: "gamma".to_string(),
            root: PathBuf::from("/skills"),
        })
    );
}

#[test]
fn restore_on_an_active_skill_is_a_no_op_with_a_hint() {
    let mut model = model_with(vec![row("alpha", SkillKind::Active)]);
    assert_eq!(model.update(key(KeyCode::Char('u'))), Flow::Stay);
    assert!(model.status.is_some());
}

#[test]
fn help_toggles_open_and_any_key_closes_it() {
    let mut model = model_with(vec![row("alpha", SkillKind::Active)]);
    assert!(!model.show_help);
    assert_eq!(model.update(key(KeyCode::Char('?'))), Flow::Stay);
    assert!(model.show_help);
    // Any key (even Esc) closes it without quitting or acting.
    assert_eq!(model.update(key(KeyCode::Esc)), Flow::Stay);
    assert!(!model.show_help);
    // The Esc was consumed by closing help, so the model is still on the list.
    assert_eq!(model.screen, Screen::List);
}

#[test]
fn help_swallows_the_first_key_so_disable_does_not_fire() {
    let mut model = model_with(vec![row("alpha", SkillKind::Active)]);
    model.update(key(KeyCode::Char('?')));
    // `d` only closes help; no disable confirmation is armed.
    assert_eq!(model.update(key(KeyCode::Char('d'))), Flow::Stay);
    assert!(!model.show_help);
    assert!(model.pending_disable.is_none());
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

#[test]
fn consolidate_arms_disable_for_a_near_duplicate_then_archives_on_y() {
    let mut model = model_with(vec![dup_row("ship-staging", "deploy-staging", 88)]);
    // `c` on a near-duplicate arms a confirm (does not mutate yet).
    assert_eq!(model.update(key(KeyCode::Char('c'))), Flow::Stay);
    assert!(model.pending_disable.is_some());
    let status = model.status.as_deref().unwrap_or_default();
    assert!(
        status.contains("near-duplicate of deploy-staging"),
        "status: {status}"
    );
    assert!(status.contains("88%"), "status: {status}");
    // `y` archives the duplicate via the existing Disable path (keeps the survivor).
    assert_eq!(
        model.update(key(KeyCode::Char('y'))),
        Flow::Mutate(Mutation::Disable {
            name: "ship-staging".to_string(),
            root: PathBuf::from("/skills"),
        })
    );
}

#[test]
fn consolidate_on_a_non_duplicate_hints_to_enable_the_curator_when_off() {
    // Curator disabled (default) → the hint points at the toggle.
    let mut model = model_with(vec![row("alpha", SkillKind::Active)]);
    assert_eq!(model.update(key(KeyCode::Char('c'))), Flow::Stay);
    assert!(model.pending_disable.is_none());
    assert!(
        model
            .status
            .as_deref()
            .unwrap_or_default()
            .contains("agent.behavior.curator"),
        "status: {:?}",
        model.status
    );
}

#[test]
fn consolidate_on_a_non_duplicate_says_so_when_curator_enabled() {
    let mut model = model_with(vec![row("alpha", SkillKind::Active)]);
    model.set_curator_enabled(true);
    assert_eq!(model.update(key(KeyCode::Char('c'))), Flow::Stay);
    assert!(
        model
            .status
            .as_deref()
            .unwrap_or_default()
            .contains("not a near-duplicate"),
        "status: {:?}",
        model.status
    );
}

#[test]
fn consolidate_from_detail_arms_confirm() {
    let mut model = model_with(vec![dup_row("ship-staging", "deploy-staging", 75)]);
    model.update(key(KeyCode::Enter));
    assert_eq!(model.screen, Screen::Detail);
    assert_eq!(model.update(key(KeyCode::Char('c'))), Flow::Stay);
    assert!(model.pending_disable.is_some());
}
