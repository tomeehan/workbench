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

/// Check if we're currently inside a tmux session
pub fn is_inside_tmux() -> bool {
    std::env::var("TMUX").is_ok_and(|v| !v.is_empty())
}

/// Attach to an existing tmux session (blocking)
/// Uses switch-client if already inside tmux, otherwise uses attach-session
pub fn attach_session(name: &str) -> Result<ExitStatus> {
    let args = if is_inside_tmux() {
        vec!["switch-client", "-t", name]
    } else {
        vec!["attach-session", "-t", name]
    };

    let status = Command::new("tmux")
        .args(&args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
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

/// List tmux sessions for a specific project
pub fn list_project_sessions(project_id: i64) -> Vec<String> {
    let prefix = format!("workbench-{}-", project_id);
    let output = Command::new("tmux")
        .args(["list-sessions", "-F", "#{session_name}"])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter(|name| name.starts_with(&prefix))
                .map(String::from)
                .collect()
        }
        _ => Vec::new(),
    }
}

/// Kill a tmux session by name
pub fn kill_session(name: &str) -> bool {
    Command::new("tmux")
        .args(["kill-session", "-t", name])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Capture the content of a tmux pane
pub fn capture_pane_content(name: &str) -> Option<String> {
    let output = Command::new("tmux")
        .args(["capture-pane", "-t", name, "-p"])
        .output()
        .ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        None
    }
}

/// Get the current working directory of a tmux pane
pub fn get_pane_cwd(name: &str) -> Option<String> {
    let output = Command::new("tmux")
        .args(["display-message", "-t", name, "-p", "#{pane_current_path}"])
        .output()
        .ok()?;
    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if path.is_empty() {
            None
        } else {
            Some(path)
        }
    } else {
        None
    }
}

/// Get the git branch name for a tmux session's current directory
pub fn get_git_branch(name: &str) -> Option<String> {
    let cwd = get_pane_cwd(name)?;
    let output = Command::new("git")
        .args(["-C", &cwd, "rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()?;
    if output.status.success() {
        let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if branch.is_empty() {
            None
        } else {
            Some(branch)
        }
    } else {
        None
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
