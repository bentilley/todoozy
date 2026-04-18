use std::ffi::OsString;
use std::path::Path;
use std::process::Command;

pub struct EditorCommand {
    editor: OsString,
    editor_name: String,
    args: Vec<OsString>,
}

impl EditorCommand {
    pub fn from_env() -> Result<Self, Box<dyn std::error::Error>> {
        let editor = std::env::var_os("EDITOR")
            .or_else(|| std::env::var_os("VISUAL"))
            .unwrap_or_else(|| OsString::from("vi"));

        let editor_name = std::path::Path::new(&editor)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| editor.to_string_lossy().into_owned());

        Ok(EditorCommand {
            editor,
            editor_name,
            args: vec![],
        })
    }

    pub fn with_location(mut self, file_path: impl AsRef<Path>, line_num: usize) -> Self {
        let file_path = file_path.as_ref();
        self.args = match self.editor_name.as_str() {
            // Vim-style editors: +<line> <file>
            "vi" | "vim" | "nvim" | "neovim" | "gvim" | "mvim" => {
                vec![OsString::from(format!("+{}", line_num)), file_path.as_os_str().to_os_string()]
            }
            // Emacs-style: +<line> <file>
            "emacs" | "emacsclient" => {
                vec![OsString::from(format!("+{}", line_num)), file_path.as_os_str().to_os_string()]
            }
            // VS Code style: --goto <file>:<line> --wait
            "code" | "code-insiders" => vec![
                OsString::from("--goto"),
                file_at_line(file_path, line_num),
                OsString::from("--wait"),
            ],
            // Nano: +<line> <file>
            "nano" => {
                vec![OsString::from(format!("+{}", line_num)), file_path.as_os_str().to_os_string()]
            }
            // Sublime Text: <file>:<line> --wait
            "subl" | "sublime_text" => {
                vec![file_at_line(file_path, line_num), OsString::from("--wait")]
            }
            // Default: try +<line> syntax (works for many editors)
            _ => vec![OsString::from(format!("+{}", line_num)), file_path.as_os_str().to_os_string()],
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
                return Err(format!(
                    "Failed to launch editor '{}': {}",
                    self.editor.to_string_lossy(),
                    e
                )
                .into());
            }
        }

        Ok(())
    }
}

fn file_at_line(file_path: &Path, line_num: usize) -> OsString {
    let mut arg = file_path.as_os_str().to_os_string();
    arg.push(":");
    arg.push(line_num.to_string());
    arg
}
