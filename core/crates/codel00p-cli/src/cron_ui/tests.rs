use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::model::{CreateStep, CronModel, CronRow, Flow, JobDetail, Mutation, Screen};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::empty())
}

fn row(id: &str) -> CronRow {
    CronRow {
        id: id.to_string(),
        schedule: "every 30m".to_string(),
        enabled: true,
        action: "Sweep the inbox".to_string(),
        detail: JobDetail {
            schedule_spec: "30m".to_string(),
            prompt: "Sweep the inbox".to_string(),
            command: None,
            workspace: None,
            provider: None,
            model: None,
            last_run_epoch: None,
        },
    }
}

fn model_with_rows() -> CronModel {
    let mut model = CronModel::new();
    model.set_rows(vec![row("cron-1")]);
    model
}

#[test]
fn enter_opens_detail_for_selected_row() {
    let mut model = model_with_rows();
    assert_eq!(
        model.update(key(KeyCode::Enter)),
        Flow::OpenDetail("cron-1".to_string())
    );
}

#[test]
fn esc_on_empty_list_quits() {
    let mut model = CronModel::new();
    assert_eq!(model.update(key(KeyCode::Esc)), Flow::Quit);
}

#[test]
fn enable_disable_and_run_are_immediate_from_detail() {
    let mut model = model_with_rows();
    model.show_detail(row("cron-1"));
    assert_eq!(model.screen, Screen::Detail);

    assert_eq!(
        model.update(key(KeyCode::Char('e'))),
        Flow::Mutate(Mutation::SetEnabled {
            id: "cron-1".to_string(),
            enabled: true,
        })
    );
    assert_eq!(
        model.update(key(KeyCode::Char('d'))),
        Flow::Mutate(Mutation::SetEnabled {
            id: "cron-1".to_string(),
            enabled: false,
        })
    );
    assert_eq!(
        model.update(key(KeyCode::Char('R'))),
        Flow::RunNow("cron-1".to_string())
    );
}

#[test]
fn delete_from_detail_requires_confirmation() {
    let mut model = model_with_rows();
    model.show_detail(row("cron-1"));

    // `x` arms a confirmation rather than deleting immediately.
    assert_eq!(model.update(key(KeyCode::Char('x'))), Flow::Stay);
    assert_eq!(model.pending_delete.as_deref(), Some("cron-1"));
    assert!(model.status.is_some());

    // `y` confirms and emits the delete mutation.
    assert_eq!(
        model.update(key(KeyCode::Char('y'))),
        Flow::Mutate(Mutation::Delete {
            id: "cron-1".to_string(),
        })
    );
    assert!(model.pending_delete.is_none());
}

#[test]
fn delete_confirmation_cancels_on_any_other_key() {
    let mut model = model_with_rows();
    model.show_detail(row("cron-1"));
    model.update(key(KeyCode::Char('x')));
    assert_eq!(model.pending_delete.as_deref(), Some("cron-1"));

    // Any non-`y` key cancels the delete and leaves the job intact.
    assert_eq!(model.update(key(KeyCode::Char('n'))), Flow::Stay);
    assert!(model.pending_delete.is_none());
}

#[test]
fn question_mark_toggles_help_and_swallows_keys() {
    let mut model = model_with_rows();
    assert!(!model.show_help);

    // `?` opens the overlay without acting on the underlying screen.
    assert_eq!(model.update(key(KeyCode::Char('?'))), Flow::Stay);
    assert!(model.show_help);

    // While shown, any key (here Enter, which would otherwise open detail) just
    // closes the overlay and does nothing else.
    assert_eq!(model.update(key(KeyCode::Enter)), Flow::Stay);
    assert!(!model.show_help);
    assert_eq!(model.screen, Screen::List);
}

#[test]
fn esc_from_detail_returns_to_list() {
    let mut model = model_with_rows();
    model.show_detail(row("cron-1"));
    assert_eq!(model.update(key(KeyCode::Esc)), Flow::Stay);
    assert_eq!(model.screen, Screen::List);
}

#[test]
fn n_opens_the_create_flow_gathering_schedule_then_prompt() {
    let mut model = model_with_rows();
    assert_eq!(model.update(key(KeyCode::Char('n'))), Flow::Stay);
    assert_eq!(model.screen, Screen::Create);
    assert_eq!(model.create_step, CreateStep::Schedule);

    for c in "30m".chars() {
        model.update(key(KeyCode::Char(c)));
    }
    // First Enter captures the schedule and advances to the prompt step.
    assert_eq!(model.update(key(KeyCode::Enter)), Flow::Stay);
    assert_eq!(model.create_step, CreateStep::Prompt);
    assert_eq!(model.create_schedule, "30m");
    assert!(model.composer.text().is_empty());

    for c in "Sweep".chars() {
        model.update(key(KeyCode::Char(c)));
    }
    // Second Enter emits the Add mutation with both fields.
    assert_eq!(
        model.update(key(KeyCode::Enter)),
        Flow::Mutate(Mutation::Add {
            schedule: "30m".to_string(),
            prompt: "Sweep".to_string(),
        })
    );
}

#[test]
fn empty_create_field_does_not_advance() {
    let mut model = model_with_rows();
    model.update(key(KeyCode::Char('n')));
    // Enter with no schedule stays on the schedule step.
    assert_eq!(model.update(key(KeyCode::Enter)), Flow::Stay);
    assert_eq!(model.create_step, CreateStep::Schedule);
}

#[test]
fn esc_cancels_create_back_to_list() {
    let mut model = model_with_rows();
    model.update(key(KeyCode::Char('n')));
    for c in "30m".chars() {
        model.update(key(KeyCode::Char(c)));
    }
    model.update(key(KeyCode::Enter));
    assert_eq!(model.update(key(KeyCode::Esc)), Flow::Stay);
    assert_eq!(model.screen, Screen::List);
    assert!(model.create_schedule.is_empty());
}

#[test]
fn ctrl_c_quits_from_any_screen() {
    let mut model = model_with_rows();
    model.show_detail(row("cron-1"));
    let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
    assert_eq!(model.update(ctrl_c), Flow::Quit);
}
