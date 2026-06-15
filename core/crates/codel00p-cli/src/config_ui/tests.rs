use std::sync::Mutex;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tempfile::TempDir;

use super::model::{ConfigModel, Flow, ProvFocus, Screen, Section};

/// Serializes the `CODEL00P_HOME` mutation so each model is built against an
/// empty, deterministic config instead of the developer's real `~/.codel00p`.
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::empty())
}

/// Builds a model over an empty temp config home. The lock is held only while the
/// model is constructed (which reads `CODEL00P_HOME`), then released — so a single
/// test may build several models without self-deadlocking on the non-reentrant
/// `Mutex`. The returned `TempDir` keeps the directory alive for the test.
fn model(section: Section) -> (ConfigModel, TempDir) {
    let _guard = ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let dir = tempfile::tempdir().expect("tempdir");
    unsafe { std::env::set_var("CODEL00P_HOME", dir.path()) };
    let model = ConfigModel::new(dir.path(), section);
    (model, dir)
}

#[test]
fn menu_navigates_and_opens_sections() {
    let (mut m, _dir) = model(Section::Menu);
    assert_eq!(m.screen, Screen::Menu);
    assert_eq!(m.menu_cursor, 0);

    assert_eq!(m.update(key(KeyCode::Down)), Flow::Continue);
    assert_eq!(m.menu_cursor, 1);
    assert_eq!(m.update(key(KeyCode::Enter)), Flow::Continue);
    assert_eq!(m.screen, Screen::Tools); // row 1 = Tools
}

#[test]
fn tools_toggle_adds_and_removes() {
    let (mut m, _dir) = model(Section::Menu);
    m.update(key(KeyCode::Down)); // -> Tools row
    m.update(key(KeyCode::Enter)); // open Tools
    assert_eq!(m.screen, Screen::Tools);

    // Cursor starts on "read".
    m.update(key(KeyCode::Char(' ')));
    assert!(m.tool_sets.iter().any(|s| s == "read"));
    m.update(key(KeyCode::Char(' ')));
    assert!(!m.tool_sets.iter().any(|s| s == "read"));

    m.update(key(KeyCode::Esc));
    assert_eq!(m.screen, Screen::Menu);
}

#[test]
fn permissions_select_updates_mode() {
    let (mut m, _dir) = model(Section::Menu);
    m.update(key(KeyCode::Down)); // Tools
    m.update(key(KeyCode::Down)); // Permissions
    m.update(key(KeyCode::Enter)); // open Permissions
    assert_eq!(m.screen, Screen::Permissions);

    // Default is "ask" (index 1); move to "deny" (index 2) and choose it.
    m.update(key(KeyCode::Down));
    m.update(key(KeyCode::Enter));
    assert_eq!(m.permission_mode, "deny");
    assert_eq!(m.screen, Screen::Menu);
}

#[test]
fn provider_select_then_key_entry_sets_pending_key() {
    let (mut m, _dir) = model(Section::Providers);
    assert_eq!(m.screen, Screen::Providers);
    assert_eq!(m.prov_focus, ProvFocus::List);

    // Select the first provider (sorted: "anthropic", an API-key provider).
    m.update(key(KeyCode::Enter));
    assert_eq!(m.provider.as_deref(), Some("anthropic"));
    assert_eq!(m.prov_focus, ProvFocus::Key);

    // Type a key, then accept the fields back to the menu.
    m.update(key(KeyCode::Char('s')));
    m.update(key(KeyCode::Char('k')));
    assert_eq!(m.key_input, "sk");
    m.update(key(KeyCode::Enter));
    assert_eq!(m.screen, Screen::Menu);
    let (var, value) = m.pending_key.as_ref().expect("pending key");
    assert!(var.contains("ANTHROPIC"));
    assert_eq!(value, "sk");
}

#[test]
fn tab_cycles_provider_fields_and_edits_model() {
    let (mut m, _dir) = model(Section::Providers);
    m.update(key(KeyCode::Tab)); // List -> Key
    assert_eq!(m.prov_focus, ProvFocus::Key);
    m.update(key(KeyCode::Tab)); // Key -> Model
    assert_eq!(m.prov_focus, ProvFocus::Model);
    m.update(key(KeyCode::Char('x')));
    assert!(m.model_input.ends_with('x'));
    m.update(key(KeyCode::Backspace));
    assert!(!m.model_input.ends_with('x'));
}

#[test]
fn save_and_quit_flows() {
    let (mut m, _dir) = model(Section::Menu);
    assert_eq!(m.update(key(KeyCode::Char('s'))), Flow::Save);

    let (mut m2, _dir2) = model(Section::Menu);
    assert_eq!(m2.update(key(KeyCode::Char('q'))), Flow::Quit);

    // "Save & quit" / "Discard & quit" menu rows.
    let (mut m3, _dir3) = model(Section::Menu);
    for _ in 0..3 {
        m3.update(key(KeyCode::Down)); // -> row 3 (Save & quit)
    }
    assert_eq!(m3.update(key(KeyCode::Enter)), Flow::Save);
}
