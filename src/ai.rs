use color_eyre::{eyre::eyre, Result};
use std::process::Command;

/// Use Claude CLI to fill in field values based on session name and context
pub fn fill_fields(
    session_name: &str,
    fields: &[(String, String)], // (name, description) pairs
    pane_content: Option<&str>,
) -> Result<Vec<String>> {
    let fields_desc: String = fields
        .iter()
        .enumerate()
        .map(|(i, (name, desc))| {
            if desc.is_empty() {
                format!("{}. {}", i + 1, name)
            } else {
                format!("{}. {} ({})", i + 1, name, desc)
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    let context = if let Some(content) = pane_content {
        // Take last 2000 chars of pane content for context (handle UTF-8 properly)
        let char_count = content.chars().count();
        let trimmed = if char_count > 2000 {
            content.chars().skip(char_count - 2000).collect::<String>()
        } else {
            content.to_string()
        };
        format!("\n\nTerminal content:\n{}", trimmed)
    } else {
        String::new()
    };

    let prompt = format!(
        r#"Given a task/session named "{}", fill in values for these fields based on the name and any context provided. Only fill in fields where you can reasonably infer a value - leave others empty.

Fields:
{}
{}
Respond with ONLY a JSON array of strings, one value per field in order. Use empty string "" for fields you can't fill. Example: ["value1", "", "value3"]"#,
        session_name, fields_desc, context
    );

    let output = Command::new("claude")
        .args(["-p", &prompt, "--output-format", "json", "--model", "haiku"])
        .output()
        .map_err(|e| eyre!("Failed to run claude: {}", e))?;

    if !output.status.success() {
        return Err(eyre!("claude command failed"));
    }

    let response: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|_| eyre!("Failed to parse claude output as JSON"))?;

    // Extract the result text from claude's JSON output
    let text = response["result"]
        .as_str()
        .ok_or_else(|| eyre!("No result in claude output"))?;

    // Parse JSON array from the result
    let values: Vec<String> = serde_json::from_str(text.trim())
        .map_err(|_| eyre!("Failed to parse AI response as JSON array"))?;

    // Ensure we have the right number of values
    let mut result = values;
    result.resize(fields.len(), String::new());

    Ok(result)
}
