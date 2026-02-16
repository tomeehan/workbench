use color_eyre::{eyre::eyre, Result};
use serde::{Deserialize, Serialize};
use std::env;

#[derive(Serialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct ApiRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<Message>,
}

#[derive(Deserialize)]
struct ContentBlock {
    text: Option<String>,
}

#[derive(Deserialize)]
struct ApiResponse {
    content: Vec<ContentBlock>,
}

/// Use Claude to fill in field values based on session name and context
pub fn fill_fields(
    session_name: &str,
    fields: &[(String, String)], // (name, description) pairs
    pane_content: Option<&str>,
) -> Result<Vec<String>> {
    let api_key = env::var("ANTHROPIC_API_KEY")
        .map_err(|_| eyre!("ANTHROPIC_API_KEY not set"))?;

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
        // Take last 2000 chars of pane content for context
        let trimmed = if content.len() > 2000 {
            &content[content.len() - 2000..]
        } else {
            content
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

    let request = ApiRequest {
        model: "claude-3-5-haiku-latest".to_string(),
        max_tokens: 1024,
        messages: vec![Message {
            role: "user".to_string(),
            content: prompt,
        }],
    };

    let client = reqwest::blocking::Client::new();
    let response = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&request)
        .send()?;

    if !response.status().is_success() {
        return Err(eyre!("API request failed: {}", response.status()));
    }

    let api_response: ApiResponse = response.json()?;
    let text = api_response
        .content
        .first()
        .and_then(|c| c.text.as_ref())
        .ok_or_else(|| eyre!("No response content"))?;

    // Parse JSON array from response
    let values: Vec<String> = serde_json::from_str(text.trim())
        .map_err(|_| eyre!("Failed to parse AI response as JSON array"))?;

    // Ensure we have the right number of values
    let mut result = values;
    result.resize(fields.len(), String::new());

    Ok(result)
}
