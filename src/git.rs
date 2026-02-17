use color_eyre::{eyre::eyre, Result};
use std::path::Path;
use std::process::{Command, Stdio};

/// Get the root of the git repository containing the given path
pub fn get_repo_root(path: &str) -> Option<String> {
    let output = Command::new("git")
        .args(["-C", path, "rev-parse", "--show-toplevel"])
        .output()
        .ok()?;

    if output.status.success() {
        let root = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if root.is_empty() {
            None
        } else {
            Some(root)
        }
    } else {
        None
    }
}

/// Check if a branch exists in the repository
pub fn branch_exists(repo_path: &str, branch_name: &str) -> bool {
    Command::new("git")
        .args(["-C", repo_path, "show-ref", "--verify", "--quiet", &format!("refs/heads/{}", branch_name)])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Create a new git worktree
/// If the branch already exists, checks it out; otherwise creates a new branch
pub fn create_worktree(repo_path: &str, branch_name: &str, worktree_path: &str) -> Result<()> {
    // Check if worktree path already exists
    if Path::new(worktree_path).exists() {
        return Err(eyre!("Worktree path already exists: {}", worktree_path));
    }

    let status = if branch_exists(repo_path, branch_name) {
        // Branch exists, check it out in the worktree
        Command::new("git")
            .args(["-C", repo_path, "worktree", "add", worktree_path, branch_name])
            .status()?
    } else {
        // Create new branch in the worktree
        Command::new("git")
            .args(["-C", repo_path, "worktree", "add", "-b", branch_name, worktree_path])
            .status()?
    };

    if status.success() {
        Ok(())
    } else {
        Err(eyre!("Failed to create worktree"))
    }
}

/// Remove a git worktree
pub fn remove_worktree(repo_path: &str, worktree_path: &str, force: bool) -> Result<()> {
    let mut args = vec!["-C", repo_path, "worktree", "remove"];
    if force {
        args.push("--force");
    }
    args.push(worktree_path);

    let status = Command::new("git")
        .args(&args)
        .status()?;

    if status.success() {
        Ok(())
    } else {
        Err(eyre!("Failed to remove worktree"))
    }
}

/// Information about dirty state in a worktree
#[derive(Debug, Clone)]
pub struct DirtyStatus {
    pub staged: usize,
    pub unstaged: usize,
    pub untracked: usize,
}

impl DirtyStatus {
    pub fn is_dirty(&self) -> bool {
        self.staged > 0 || self.unstaged > 0 || self.untracked > 0
    }
}

/// Check if a worktree has uncommitted changes
pub fn is_worktree_dirty(path: &str) -> bool {
    get_dirty_status(path).map(|s| s.is_dirty()).unwrap_or(false)
}

/// Get detailed dirty status for a worktree
pub fn get_dirty_status(path: &str) -> Option<DirtyStatus> {
    // Check if path exists and is a git worktree
    if !Path::new(path).exists() {
        return None;
    }

    let output = Command::new("git")
        .args(["-C", path, "status", "--porcelain"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let status_output = String::from_utf8_lossy(&output.stdout);
    let mut staged = 0;
    let mut unstaged = 0;
    let mut untracked = 0;

    for line in status_output.lines() {
        if line.len() < 2 {
            continue;
        }
        let index_status = line.chars().next().unwrap_or(' ');
        let worktree_status = line.chars().nth(1).unwrap_or(' ');

        // Staged changes (index has changes)
        if index_status != ' ' && index_status != '?' {
            staged += 1;
        }
        // Unstaged changes (worktree has changes)
        if worktree_status != ' ' && worktree_status != '?' {
            unstaged += 1;
        }
        // Untracked files
        if index_status == '?' {
            untracked += 1;
        }
    }

    Some(DirtyStatus {
        staged,
        unstaged,
        untracked,
    })
}

/// Sanitize a session name into a valid git branch name
/// "Fix Auth Bug" -> "wb/fix-auth-bug"
pub fn sanitize_branch_name(session_name: &str) -> String {
    let sanitized: String = session_name
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c
            } else {
                '-'
            }
        })
        .collect();

    // Remove consecutive dashes and trim dashes from ends
    let mut result = String::new();
    let mut last_was_dash = true; // Start true to skip leading dashes
    for c in sanitized.chars() {
        if c == '-' {
            if !last_was_dash {
                result.push(c);
                last_was_dash = true;
            }
        } else {
            result.push(c);
            last_was_dash = false;
        }
    }

    // Remove trailing dash
    while result.ends_with('-') {
        result.pop();
    }

    if result.is_empty() {
        result = "session".to_string();
    }

    format!("wb/{}", result)
}

/// Generate a worktree path based on repo path and branch name
/// Repo at `/Users/tom/Code/myproject` + branch `wb/fix-auth-bug`:
/// -> `/Users/tom/Code/myproject-fix-auth-bug/`
pub fn generate_worktree_path(repo_path: &str, branch_name: &str) -> String {
    // Extract the part after "wb/" prefix
    let branch_suffix = branch_name.strip_prefix("wb/").unwrap_or(branch_name);

    format!("{}-{}", repo_path, branch_suffix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_branch_name() {
        assert_eq!(sanitize_branch_name("Fix Auth Bug"), "wb/fix-auth-bug");
        assert_eq!(sanitize_branch_name("add feature"), "wb/add-feature");
        assert_eq!(sanitize_branch_name("test--multiple---dashes"), "wb/test-multiple-dashes");
        assert_eq!(sanitize_branch_name("  leading spaces  "), "wb/leading-spaces");
        assert_eq!(sanitize_branch_name("UPPERCASE"), "wb/uppercase");
        assert_eq!(sanitize_branch_name("special!@#chars"), "wb/special-chars");
    }

    #[test]
    fn test_generate_worktree_path() {
        assert_eq!(
            generate_worktree_path("/Users/tom/Code/myproject", "wb/fix-auth-bug"),
            "/Users/tom/Code/myproject-fix-auth-bug"
        );
        assert_eq!(
            generate_worktree_path("/home/user/repo", "wb/new-feature"),
            "/home/user/repo-new-feature"
        );
    }
}
