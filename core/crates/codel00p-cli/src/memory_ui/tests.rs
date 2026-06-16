use codel00p_protocol::{MemoryKind, MemoryStatus};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::model::{Flow, MemoryModel, MemoryRow, Mutation, Screen, StatusFilter};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::empty())
}

fn row(id: &str, content: &str) -> MemoryRow {
    MemoryRow {
        id: id.to_string(),
        status: MemoryStatus::Candidate,
        kind: MemoryKind::Convention,
        content: content.to_string(),
        tags: Vec::new(),
    }
}

fn model_with_rows() -> MemoryModel {
    let mut model = MemoryModel::new("tester".to_string());
    model.set_rows(vec![row("mem-1", "Run cargo from core.")]);
    model
}

#[test]
fn tab_cycles_status_filter_and_reloads() {
    let mut model = MemoryModel::new("tester".to_string());
    assert_eq!(model.filter, StatusFilter::Candidate);
    assert_eq!(model.update(key(KeyCode::Tab)), Flow::Reload);
    assert_eq!(model.filter, StatusFilter::Approved);
    assert_eq!(model.update(key(KeyCode::BackTab)), Flow::Reload);
    assert_eq!(model.filter, StatusFilter::Candidate);
}

#[test]
fn enter_opens_detail_for_selected_row() {
    let mut model = model_with_rows();
    assert_eq!(
        model.update(key(KeyCode::Enter)),
        Flow::OpenDetail("mem-1".to_string())
    );
}

#[test]
fn approve_from_detail_is_immediate() {
    let mut model = model_with_rows();
    model.show_detail(row("mem-1", "Run cargo from core."), Vec::new());
    assert_eq!(model.screen, Screen::Detail);
    assert_eq!(
        model.update(key(KeyCode::Char('a'))),
        Flow::Mutate(Mutation::Approve {
            id: "mem-1".to_string()
        })
    );
}

#[test]
fn reject_prompts_for_reason_then_mutates() {
    let mut model = model_with_rows();
    model.show_detail(row("mem-1", "Run cargo from core."), Vec::new());

    assert_eq!(model.update(key(KeyCode::Char('r'))), Flow::Stay);
    assert_eq!(model.screen, Screen::Prompt);

    for c in "dup".chars() {
        model.update(key(KeyCode::Char(c)));
    }
    assert_eq!(
        model.update(key(KeyCode::Enter)),
        Flow::Mutate(Mutation::Reject {
            id: "mem-1".to_string(),
            reason: "dup".to_string()
        })
    );
}

#[test]
fn empty_reason_does_not_mutate() {
    let mut model = model_with_rows();
    model.show_detail(row("mem-1", "x"), Vec::new());
    model.update(key(KeyCode::Char('r')));
    // Enter with no text stays on the prompt.
    assert_eq!(model.update(key(KeyCode::Enter)), Flow::Stay);
    assert_eq!(model.screen, Screen::Prompt);
}

#[test]
fn esc_cancels_prompt_back_to_detail() {
    let mut model = model_with_rows();
    model.show_detail(row("mem-1", "x"), Vec::new());
    model.update(key(KeyCode::Char('r')));
    assert_eq!(model.update(key(KeyCode::Esc)), Flow::Stay);
    assert_eq!(model.screen, Screen::Detail);
}

#[test]
fn edit_prefills_existing_content() {
    let mut model = model_with_rows();
    model.show_detail(row("mem-1", "old fact"), Vec::new());
    model.update(key(KeyCode::Char('e')));
    assert_eq!(model.screen, Screen::Prompt);
    assert_eq!(model.composer.text(), "old fact");

    // Append to the pre-filled content, then confirm.
    model.update(key(KeyCode::Char('s')));
    assert_eq!(
        model.update(key(KeyCode::Enter)),
        Flow::Mutate(Mutation::Edit {
            id: "mem-1".to_string(),
            content: "old facts".to_string()
        })
    );
}
