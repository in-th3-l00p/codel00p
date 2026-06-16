use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::model::{Flow, Screen, SessionRow, SessionsModel};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::empty())
}

fn row(id: &str) -> SessionRow {
    SessionRow {
        id: id.to_string(),
        source: "chat".to_string(),
        messages: 4,
        events: 9,
    }
}

fn model_with_rows() -> SessionsModel {
    let mut model = SessionsModel::new();
    model.set_rows(vec![row("session-1"), row("session-2")]);
    model
}

#[test]
fn enter_opens_detail_for_selected_session() {
    let mut model = model_with_rows();
    assert_eq!(
        model.update(key(KeyCode::Enter)),
        Flow::OpenDetail("session-1".to_string())
    );
}

#[test]
fn r_on_list_resumes_selected_session() {
    let mut model = model_with_rows();
    assert_eq!(
        model.update(key(KeyCode::Char('r'))),
        Flow::Resume("session-1".to_string())
    );
}

#[test]
fn r_in_detail_resumes_open_session() {
    let mut model = model_with_rows();
    model.show_detail(row("session-2"), vec!["a".into()]);
    assert_eq!(
        model.update(key(KeyCode::Char('r'))),
        Flow::Resume("session-2".to_string())
    );
}

#[test]
fn esc_on_list_quits() {
    let mut model = model_with_rows();
    assert_eq!(model.update(key(KeyCode::Esc)), Flow::Quit);
}

#[test]
fn question_mark_toggles_help_and_swallows_keys() {
    let mut model = model_with_rows();
    assert!(!model.show_help);

    assert_eq!(model.update(key(KeyCode::Char('?'))), Flow::Stay);
    assert!(model.show_help);

    // Enter would normally open detail; while help is up it just closes the overlay.
    assert_eq!(model.update(key(KeyCode::Enter)), Flow::Stay);
    assert!(!model.show_help);
    assert_eq!(model.screen, Screen::List);
}

#[test]
fn detail_scrolls_and_esc_returns_to_list() {
    let mut model = model_with_rows();
    model.show_detail(row("session-1"), vec!["a".into(), "b".into(), "c".into()]);
    assert_eq!(model.screen, Screen::Detail);

    assert_eq!(model.update(key(KeyCode::Down)), Flow::Stay);
    assert_eq!(model.scroll, 1);
    assert_eq!(model.update(key(KeyCode::Up)), Flow::Stay);
    assert_eq!(model.scroll, 0);
    // Scroll is clamped to the transcript length.
    for _ in 0..10 {
        model.update(key(KeyCode::Down));
    }
    assert_eq!(model.scroll, 2);

    model.update(key(KeyCode::Esc));
    assert_eq!(model.screen, Screen::List);
    assert!(model.transcript.is_empty());
}
