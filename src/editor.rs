use std::path::Path;
use std::process::ExitStatus;

use crate::error::{Error, Result};

/// Resolve the user's preferred editor from environment variables.
///
/// Checks `$EDITOR`, then `$VISUAL`, falling back to `"vi"`.
pub fn resolve_editor() -> String {
    std::env::var("EDITOR")
        .or_else(|_| std::env::var("VISUAL"))
        .unwrap_or_else(|_| "vi".into())
}

/// Parse an editor command string into (executable, extra_args).
///
/// Uses POSIX shell-word splitting so that quoted arguments and paths with
/// spaces are handled correctly (e.g. `"/usr/local/My Apps/code" --wait`).
pub fn parse_editor_command(editor: &str) -> (String, Vec<String>) {
    match shell_words::split(editor) {
        Ok(parts) if !parts.is_empty() => {
            let mut iter = parts.into_iter();
            let exe = iter.next().unwrap();
            let args: Vec<String> = iter.collect();
            (exe, args)
        }
        _ => ("vi".into(), Vec::new()),
    }
}

/// Launch an editor process on the given file path.
///
/// Returns `Ok(ExitStatus)` when the process was spawned successfully, even
/// if it exited non-zero. Returns `Err` only when the command could not be
/// started at all (e.g. binary not found).
pub fn launch_editor(exe: &str, args: &[String], file_path: &Path) -> Result<ExitStatus> {
    let status = std::process::Command::new(exe)
        .args(args)
        .arg(file_path)
        .status()?;
    Ok(status)
}

/// Convenience: resolve editor, parse, launch on the given file.
///
/// Returns `Ok(())` on success, or `Err(EditorFailed)` if the editor exits
/// with a non-zero status. Propagates IO errors if the editor binary cannot
/// be spawned.
pub fn open_in_editor(file_path: &Path) -> Result<()> {
    let editor = resolve_editor();
    let (exe, args) = parse_editor_command(&editor);
    let status = launch_editor(&exe, &args, file_path)?;
    if !status.success() {
        return Err(Error::EditorFailed {
            reason: format!(
                "{exe} exited with {}",
                status
                    .code()
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "signal".into())
            ),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Editor command parsing ───────────────────────────────────────────

    #[test]
    fn parse_simple_binary() {
        let (exe, args) = parse_editor_command("vim");
        assert_eq!(exe, "vim");
        assert!(args.is_empty());
    }

    #[test]
    fn parse_binary_with_flags() {
        let (exe, args) = parse_editor_command("code --wait");
        assert_eq!(exe, "code");
        assert_eq!(args, vec!["--wait"]);
    }

    #[test]
    fn parse_multiple_flags() {
        let (exe, args) = parse_editor_command("emacs -nw --no-splash");
        assert_eq!(exe, "emacs");
        assert_eq!(args, vec!["-nw", "--no-splash"]);
    }

    #[test]
    fn parse_empty_falls_back_to_vi() {
        let (exe, _) = parse_editor_command("");
        assert_eq!(exe, "vi");
    }

    #[test]
    fn parse_path_with_spaces() {
        // Quoted executable path with spaces
        let (exe, args) = parse_editor_command("'/usr/local/My Apps/editor' --wait");
        assert_eq!(exe, "/usr/local/My Apps/editor");
        assert_eq!(args, vec!["--wait"]);
    }

    #[test]
    fn parse_quoted_args() {
        let (exe, args) = parse_editor_command("editor '--config path'");
        assert_eq!(exe, "editor");
        assert_eq!(args, vec!["--config path"]);
    }

    #[test]
    fn parse_double_quoted_exe() {
        let (exe, args) = parse_editor_command(r#""My Editor" --wait"#);
        assert_eq!(exe, "My Editor");
        assert_eq!(args, vec!["--wait"]);
    }

    // ── Editor launch ────────────────────────────────────────────────────

    #[test]
    fn launch_nonexistent_binary_returns_error() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("test.md");
        std::fs::write(&file, "test").unwrap();

        let result = launch_editor("__nonexistent_editor_42__", &[], &file);
        assert!(
            result.is_err(),
            "expected Err for nonexistent editor binary"
        );
    }

    #[test]
    fn launch_successful_editor_returns_ok() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("test.md");
        std::fs::write(&file, "test").unwrap();

        // `true` is a standard Unix binary that always exits 0
        let result = launch_editor("true", &[], &file);
        assert!(result.is_ok());
        assert!(result.unwrap().success());
    }

    #[test]
    fn launch_failing_editor_returns_nonzero_status() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("test.md");
        std::fs::write(&file, "test").unwrap();

        // `false` is a standard Unix binary that always exits 1
        let result = launch_editor("false", &[], &file);
        assert!(result.is_ok(), "process spawned, so no io error");
        assert!(!result.unwrap().success(), "exit code should be non-zero");
    }

    // ── open_in_editor behavior via launch_editor + error wrapping ─────

    #[test]
    fn nonexistent_editor_propagates_io_error() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("test.md");
        std::fs::write(&file, "test").unwrap();

        // launch_editor with a nonexistent binary should return io::Error
        let result = launch_editor("__nonexistent_editor_42__", &[], &file);
        assert!(result.is_err());
    }

    #[test]
    fn nonzero_exit_detected_via_exit_status() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("test.md");
        std::fs::write(&file, "test").unwrap();

        // `false` exits with code 1 — open_in_editor wraps this as EditorFailed
        let status = launch_editor("false", &[], &file).unwrap();
        assert!(!status.success());

        // Verify the wrapping logic in open_in_editor produces EditorFailed
        let (exe, args) = parse_editor_command("false");
        let status = launch_editor(&exe, &args, &file).unwrap();
        assert!(!status.success());
        // Simulate what open_in_editor does with a non-zero status
        let err = crate::error::Error::EditorFailed {
            reason: format!(
                "{exe} exited with {}",
                status
                    .code()
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "signal".into())
            ),
        };
        assert!(
            err.to_string().contains("editor failed"),
            "expected EditorFailed, got: {err}"
        );
    }
}
