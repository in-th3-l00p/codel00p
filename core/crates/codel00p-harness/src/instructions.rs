use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{errors::HarnessError, workspace::Workspace};

const INSTRUCTION_FILES: [&str; 3] = ["CODEL00P.md", "AGENTS.md", "CLAUDE.md"];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectInstruction {
    source: String,
    content: String,
}

impl ProjectInstruction {
    pub fn new(source: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            content: normalize_instruction_content(content.into()),
        }
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn content(&self) -> &str {
        &self.content
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectInstructions {
    instructions: Vec<ProjectInstruction>,
}

impl ProjectInstructions {
    pub fn new(instructions: Vec<ProjectInstruction>) -> Self {
        Self { instructions }
    }

    pub fn is_empty(&self) -> bool {
        self.instructions.is_empty()
    }

    pub fn items(&self) -> &[ProjectInstruction] {
        &self.instructions
    }

    pub fn sources(&self) -> Vec<&str> {
        self.instructions
            .iter()
            .map(ProjectInstruction::source)
            .collect()
    }

    pub fn as_prompt(&self) -> String {
        let mut prompt = String::from("Project instructions:");
        for instruction in &self.instructions {
            prompt.push_str("\n## ");
            prompt.push_str(instruction.source());
            prompt.push('\n');
            prompt.push_str(instruction.content());
            prompt.push('\n');
        }
        prompt.pop();
        prompt
    }
}

#[derive(Clone, Debug, Default)]
pub struct ProjectInstructionLoader;

impl ProjectInstructionLoader {
    pub fn load(&self, workspace: &Workspace) -> Result<ProjectInstructions, HarnessError> {
        let mut instructions = Vec::new();
        for file_name in INSTRUCTION_FILES {
            let path = workspace.root().join(file_name);
            if !path.exists() {
                continue;
            }
            if !path.is_file() {
                continue;
            }
            let content = read_instruction_file(&path)?;
            if content.trim().is_empty() {
                continue;
            }
            instructions.push(ProjectInstruction::new(file_name, content));
        }

        Ok(ProjectInstructions::new(instructions))
    }
}

fn read_instruction_file(path: &Path) -> Result<String, HarnessError> {
    Ok(std::fs::read_to_string(path)?)
}

fn normalize_instruction_content(content: String) -> String {
    content.trim().replace("\r\n", "\n")
}
