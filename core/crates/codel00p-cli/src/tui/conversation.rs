//! The chat transcript model: an ordered list of blocks the view renders. This is
//! pure state — no terminal or I/O — so its behavior is unit-tested directly.

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ToolState {
    Requested,
    Running(Option<String>),
    Done,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Block {
    User(String),
    Assistant(String),
    Tool { name: String, state: ToolState },
    Notice(String),
    Error(String),
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Conversation {
    pub(crate) blocks: Vec<Block>,
}

impl Conversation {
    pub(crate) fn push_user(&mut self, text: impl Into<String>) {
        self.blocks.push(Block::User(text.into()));
    }

    pub(crate) fn push_notice(&mut self, text: impl Into<String>) {
        self.blocks.push(Block::Notice(text.into()));
    }

    pub(crate) fn push_error(&mut self, text: impl Into<String>) {
        self.blocks.push(Block::Error(text.into()));
    }

    /// Appends a streamed token, growing the current assistant block or starting a
    /// new one when the previous block was something else (e.g. a tool call).
    pub(crate) fn append_token(&mut self, token: &str) {
        match self.blocks.last_mut() {
            Some(Block::Assistant(text)) => text.push_str(token),
            _ => self.blocks.push(Block::Assistant(token.to_string())),
        }
    }

    /// Ensures the assistant's final message is shown even if nothing streamed
    /// (e.g. a non-streaming provider, or a turn that only called tools).
    pub(crate) fn finalize_assistant(&mut self, message: &str) {
        if message.is_empty() {
            return;
        }
        match self.blocks.last() {
            Some(Block::Assistant(text)) if !text.is_empty() => {}
            _ => self.blocks.push(Block::Assistant(message.to_string())),
        }
    }

    pub(crate) fn tool_requested(&mut self, name: &str) {
        self.blocks.push(Block::Tool {
            name: name.to_string(),
            state: ToolState::Requested,
        });
    }

    pub(crate) fn tool_progress(&mut self, name: &str, message: Option<String>) {
        self.set_tool_state(name, ToolState::Running(message));
    }

    pub(crate) fn tool_completed(&mut self, name: &str) {
        self.set_tool_state(name, ToolState::Done);
    }

    pub(crate) fn tool_failed(&mut self, name: &str, message: &str) {
        self.set_tool_state(name, ToolState::Failed(message.to_string()));
    }

    /// Updates the most recent block for `name`, or appends one if none exists yet
    /// (progress can arrive before the request block in some orderings).
    fn set_tool_state(&mut self, name: &str, state: ToolState) {
        for block in self.blocks.iter_mut().rev() {
            if let Block::Tool {
                name: tool_name,
                state: tool_state,
            } = block
                && tool_name == name
            {
                *tool_state = state;
                return;
            }
        }
        self.blocks.push(Block::Tool {
            name: name.to_string(),
            state,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_grow_one_assistant_block() {
        let mut conversation = Conversation::default();
        conversation.append_token("Hel");
        conversation.append_token("lo");
        assert_eq!(conversation.blocks, vec![Block::Assistant("Hello".into())]);
    }

    #[test]
    fn token_after_tool_starts_new_assistant_block() {
        let mut conversation = Conversation::default();
        conversation.append_token("before");
        conversation.tool_requested("read_file");
        conversation.append_token("after");
        assert_eq!(
            conversation.blocks,
            vec![
                Block::Assistant("before".into()),
                Block::Tool {
                    name: "read_file".into(),
                    state: ToolState::Requested
                },
                Block::Assistant("after".into()),
            ]
        );
    }

    #[test]
    fn tool_lifecycle_updates_same_block() {
        let mut conversation = Conversation::default();
        conversation.tool_requested("run_command");
        conversation.tool_progress("run_command", Some("running".into()));
        conversation.tool_completed("run_command");
        assert_eq!(
            conversation.blocks,
            vec![Block::Tool {
                name: "run_command".into(),
                state: ToolState::Done
            }]
        );
    }

    #[test]
    fn finalize_does_not_duplicate_streamed_text() {
        let mut conversation = Conversation::default();
        conversation.append_token("streamed answer");
        conversation.finalize_assistant("streamed answer");
        assert_eq!(
            conversation.blocks,
            vec![Block::Assistant("streamed answer".into())]
        );
    }

    #[test]
    fn finalize_adds_message_when_nothing_streamed() {
        let mut conversation = Conversation::default();
        conversation.tool_requested("git_status");
        conversation.tool_completed("git_status");
        conversation.finalize_assistant("done");
        assert_eq!(
            conversation.blocks.last(),
            Some(&Block::Assistant("done".into()))
        );
    }
}
