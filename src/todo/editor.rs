use std::process::Command;

pub struct EditorCommand {
    editor: String,
    editor_name: String,
    args: Vec<String>,
}

impl EditorCommand {
    pub fn from_env() -> Result<Self, Box<dyn std::error::Error>> {
        let editor = std::env::var("EDITOR")
            .or_else(|_| std::env::var("VISUAL"))
            .unwrap_or_else(|_| "vi".to_string());

        let editor_name = std::path::Path::new(&editor)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&editor)
            .to_string();

        Ok(EditorCommand {
            editor,
            editor_name,
            args: vec![],
        })
    }

    pub fn with_location(mut self, file_path: &str, line_num: usize) -> Self {
        self.args = match self.editor_name.as_str() {
            // Vim-style editors: +<line> <file>
            "vi" | "vim" | "nvim" | "neovim" | "gvim" | "mvim" => {
                vec![format!("+{}", line_num), file_path.to_string()]
            }
            // Emacs-style: +<line> <file>
            "emacs" | "emacsclient" => vec![format!("+{}", line_num), file_path.to_string()],
            // VS Code style: --goto <file>:<line> --wait
            "code" | "code-insiders" => vec![
                "--goto".to_string(),
                format!("{}:{}", file_path, line_num),
                "--wait".to_string(),
            ],
            // Nano: +<line> <file>
            "nano" => vec![format!("+{}", line_num), file_path.to_string()],
            // Sublime Text: <file>:<line> --wait
            "subl" | "sublime_text" => {
                vec![format!("{}:{}", file_path, line_num), "--wait".to_string()]
            }
            // Default: try +<line> syntax (works for many editors)
            _ => vec![format!("+{}", line_num), file_path.to_string()],
        };

        self
    }

    pub fn execute(&self) -> Result<(), Box<dyn std::error::Error>> {
        let status = Command::new(&self.editor).args(&self.args).status();

        match status {
            Ok(exit_status) => {
                if !exit_status.success() {
                    return Err(format!(
                        "Editor exited with code {}",
                        exit_status.code().unwrap_or(1)
                    )
                    .into());
                }
            }
            Err(e) => {
                return Err(format!("Failed to launch editor '{}': {}", self.editor, e).into());
            }
        }

        Ok(())
    }
}
