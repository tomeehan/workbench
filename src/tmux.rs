use color_eyre::{eyre::eyre, Result};
use std::process::{Command, ExitStatus, Stdio};

/// Check if tmux is installed and available
pub fn is_available() -> bool {
    Command::new("tmux")
        .arg("-V")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Generate a tmux session name for a workbench session
pub fn session_name(project_id: i64, session_id: i64) -> String {
    format!("workbench-{}-{}", project_id, session_id)
}

/// Check if a tmux session with the given name exists
pub fn session_exists(name: &str) -> bool {
    Command::new("tmux")
        .args(["has-session", "-t", name])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Create a new tmux session with a shell in the specified working directory
pub fn create_session(name: &str, working_dir: &str) -> Result<()> {
    let status = Command::new("tmux")
        .args([
            "new-session",
            "-d",           // detached
            "-s", name,     // session name
            "-c", working_dir, // start directory
        ])
        .status()?;

    if status.success() {
        Ok(())
    } else {
        Err(eyre!("Failed to create tmux session"))
    }
}

/// Attach to an existing tmux session (blocking)
pub fn attach_session(name: &str) -> Result<ExitStatus> {
    let status = Command::new("tmux")
        .args(["attach-session", "-t", name])
        .status()?;

    Ok(status)
}

/// List all workbench tmux sessions
pub fn list_workbench_sessions() -> Vec<String> {
    let output = Command::new("tmux")
        .args(["list-sessions", "-F", "#{session_name}"])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter(|name| name.starts_with("workbench-"))
                .map(String::from)
                .collect()
        }
        _ => Vec::new(),
    }
}

/// Check if a tmux session is waiting for user input by examining pane content
pub fn is_waiting_for_input(name: &str) -> bool {
    let output = Command::new("tmux")
        .args(["capture-pane", "-t", name, "-p"])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            let content = String::from_utf8_lossy(&output.stdout);
            // Check last few lines for Claude Code input prompts
            let last_lines: String = content.lines().rev().take(5).collect::<Vec<_>>().join("\n");

            // Common Claude Code input prompt patterns
            last_lines.contains("Enter to select")
                || last_lines.contains("Do you want to")
                || last_lines.contains("yes/yes to all/no")
                || last_lines.contains("Allow once")
                || last_lines.contains("Allow always")
                || last_lines.contains("(y/n)")
                || last_lines.contains("[Y/n]")
                || last_lines.contains("[y/N]")
        }
        _ => false,
    }
}
