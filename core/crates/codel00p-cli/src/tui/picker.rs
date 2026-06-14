//! A generic, terminal-independent selectable list with incremental filtering.
//! Every overlay menu (models, projects, agents, MCP servers, memory) is a
//! `Picker<T>`, so navigation and filtering behavior is defined and tested once.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// How an item presents itself in a picker. `label` is also what filtering matches
/// against.
pub(crate) trait PickerItem {
    fn label(&self) -> String;
    fn detail(&self) -> Option<String> {
        None
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PickerOutcome {
    /// The key was consumed; nothing selected yet.
    Pending,
    /// The user pressed Enter on the highlighted row.
    Selected,
    /// The user dismissed the picker (Esc).
    Cancelled,
}

#[derive(Clone, Debug)]
pub(crate) struct Picker<T> {
    items: Vec<T>,
    query: String,
    filtered: Vec<usize>,
    selected: usize,
}

impl<T: PickerItem> Picker<T> {
    pub(crate) fn new(items: Vec<T>) -> Self {
        let mut picker = Self {
            items,
            query: String::new(),
            filtered: Vec::new(),
            selected: 0,
        };
        picker.refilter();
        picker
    }

    pub(crate) fn set_items(&mut self, items: Vec<T>) {
        self.items = items;
        self.refilter();
    }

    pub(crate) fn query(&self) -> &str {
        &self.query
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// The visible rows after filtering, paired with their selected flag.
    pub(crate) fn visible(&self) -> impl Iterator<Item = (&T, bool)> {
        self.filtered
            .iter()
            .enumerate()
            .map(move |(row, &index)| (&self.items[index], row == self.selected))
    }

    pub(crate) fn selected_item(&self) -> Option<&T> {
        self.filtered
            .get(self.selected)
            .map(|&index| &self.items[index])
    }

    pub(crate) fn on_key(&mut self, key: KeyEvent) -> PickerOutcome {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc => PickerOutcome::Cancelled,
            KeyCode::Enter => {
                if self.selected_item().is_some() {
                    PickerOutcome::Selected
                } else {
                    PickerOutcome::Pending
                }
            }
            KeyCode::Up => self.move_up(),
            KeyCode::Down => self.move_down(),
            KeyCode::Char('p') if ctrl => self.move_up(),
            KeyCode::Char('n') if ctrl => self.move_down(),
            KeyCode::PageUp => self.move_by(-5),
            KeyCode::PageDown => self.move_by(5),
            KeyCode::Backspace => {
                self.query.pop();
                self.refilter();
                PickerOutcome::Pending
            }
            KeyCode::Char(c) if !ctrl => {
                self.query.push(c);
                self.refilter();
                PickerOutcome::Pending
            }
            _ => PickerOutcome::Pending,
        }
    }

    fn move_up(&mut self) -> PickerOutcome {
        self.selected = self.selected.saturating_sub(1);
        PickerOutcome::Pending
    }

    fn move_down(&mut self) -> PickerOutcome {
        if self.selected + 1 < self.filtered.len() {
            self.selected += 1;
        }
        PickerOutcome::Pending
    }

    fn move_by(&mut self, delta: isize) -> PickerOutcome {
        let len = self.filtered.len() as isize;
        if len == 0 {
            return PickerOutcome::Pending;
        }
        let next = (self.selected as isize + delta).clamp(0, len - 1);
        self.selected = next as usize;
        PickerOutcome::Pending
    }

    fn refilter(&mut self) {
        let needle = self.query.to_lowercase();
        self.filtered = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| needle.is_empty() || item.label().to_lowercase().contains(&needle))
            .map(|(index, _)| index)
            .collect();
        if self.selected >= self.filtered.len() {
            self.selected = self.filtered.len().saturating_sub(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Row(&'static str);
    impl PickerItem for Row {
        fn label(&self) -> String {
            self.0.to_string()
        }
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn picker() -> Picker<Row> {
        Picker::new(vec![Row("alpha"), Row("beta"), Row("gamma")])
    }

    #[test]
    fn arrows_move_and_clamp() {
        let mut picker = picker();
        assert_eq!(picker.selected_item().map(|row| row.0), Some("alpha"));
        picker.on_key(key(KeyCode::Up)); // already at top, stays
        assert_eq!(picker.selected_item().map(|row| row.0), Some("alpha"));
        picker.on_key(key(KeyCode::Down));
        picker.on_key(key(KeyCode::Down));
        picker.on_key(key(KeyCode::Down)); // clamps at bottom
        assert_eq!(picker.selected_item().map(|row| row.0), Some("gamma"));
    }

    #[test]
    fn typing_filters_and_resets_overflow_selection() {
        let mut picker = picker();
        picker.on_key(key(KeyCode::Down));
        picker.on_key(key(KeyCode::Down)); // gamma
        for c in "be".chars() {
            picker.on_key(key(KeyCode::Char(c)));
        }
        assert_eq!(picker.query(), "be");
        assert_eq!(picker.selected_item().map(|row| row.0), Some("beta"));
        assert_eq!(picker.visible().count(), 1);
    }

    #[test]
    fn enter_selects_and_esc_cancels() {
        let mut picker = picker();
        assert_eq!(picker.on_key(key(KeyCode::Enter)), PickerOutcome::Selected);
        assert_eq!(picker.on_key(key(KeyCode::Esc)), PickerOutcome::Cancelled);
    }

    #[test]
    fn enter_on_empty_filter_is_pending() {
        let mut picker = picker();
        for c in "zzz".chars() {
            picker.on_key(key(KeyCode::Char(c)));
        }
        assert!(picker.selected_item().is_none());
        assert_eq!(picker.on_key(key(KeyCode::Enter)), PickerOutcome::Pending);
    }
}
