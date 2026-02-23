use anyhow::Result;
use tracing::warn;

/// A single directed edge to upsert into the knowledge graph.
///
/// If `src_kind`/`src_value` are `None`, the edge is from "the current frame"
/// (stored with `src_node_id = NULL` in `kg_edges`).
#[derive(Debug, Clone)]
pub struct ExtractedEdge {
    pub src_kind:  Option<String>,
    pub src_value: Option<String>,
    pub relation:  String,
    pub dst_kind:  String,
    pub dst_value: String,
}

impl ExtractedEdge {
    fn frameless(relation: &str, dst_kind: &str, dst_value: &str) -> Self {
        Self {
            src_kind:  None,
            src_value: None,
            relation:  relation.to_string(),
            dst_kind:  dst_kind.to_string(),
            dst_value: dst_value.to_string(),
        }
    }

    fn with_src(src_kind: &str, src_value: &str, relation: &str, dst_kind: &str, dst_value: &str) -> Self {
        Self {
            src_kind:  Some(src_kind.to_string()),
            src_value: Some(src_value.to_string()),
            relation:  relation.to_string(),
            dst_kind:  dst_kind.to_string(),
            dst_value: dst_value.to_string(),
        }
    }
}

/// Rule-based extraction from structured metadata fields.
/// Synchronous, zero-cost, 100% accurate for what it covers.
///
/// Produces:
/// - `metadata.url`            → VISITED edge  + BELONGS_TO_DOMAIN edge
/// - `metadata.cwd`            → WORKING_IN edge
/// - `metadata.foreground_cmd` → RAN edge
pub fn from_metadata(_app_name: &str, metadata: &serde_json::Value) -> Vec<ExtractedEdge> {
    let mut edges = Vec::new();

    if let Some(url) = metadata.get("url").and_then(|v| v.as_str()) {
        let url = truncate(url, 300);
        if !url.is_empty() {
            edges.push(ExtractedEdge::frameless("VISITED", "URL", url));

            // Extract domain from URL
            if let Some(domain) = extract_domain(url) {
                edges.push(ExtractedEdge::with_src("URL", url, "BELONGS_TO_DOMAIN", "DOMAIN", domain));
            }
        }
    }

    if let Some(cwd) = metadata.get("cwd").and_then(|v| v.as_str()) {
        let cwd = truncate(cwd, 300);
        if !cwd.is_empty() {
            edges.push(ExtractedEdge::frameless("WORKING_IN", "DIRECTORY", cwd));
        }
    }

    if let Some(cmd) = metadata.get("foreground_cmd").and_then(|v| v.as_str()) {
        let cmd = truncate(cmd, 300);
        if !cmd.is_empty() {
            edges.push(ExtractedEdge::frameless("RAN", "COMMAND", cmd));
        }
    }

    edges
}

/// LLM-based NER via Ollama (local DeepSeek or any model).
/// Returns empty Vec on any error (non-fatal — rule-based edges always written).
pub async fn from_ocr_llm(
    app_name: &str,
    window_title: &str,
    ocr_text: &str,
    ollama_endpoint: &str,
    ollama_model: &str,
) -> Vec<ExtractedEdge> {
    match call_ollama_ner(app_name, window_title, ocr_text, ollama_endpoint, ollama_model).await {
        Ok(edges) => edges,
        Err(e) => {
            warn!("LLM NER failed (non-fatal): {e:#}");
            Vec::new()
        }
    }
}

async fn call_ollama_ner(
    app_name: &str,
    window_title: &str,
    ocr_text: &str,
    ollama_endpoint: &str,
    ollama_model: &str,
) -> Result<Vec<ExtractedEdge>> {
    let system_prompt = "You are an entity extractor. Given a screen capture's OCR text and \
app context, extract named entities as a JSON array. Each entity has a \"type\" (open string — \
use whatever label best describes it) and a \"value\".\n\n\
Focus on things that aid recall: errors, commands, issues, decisions, URLs, \
file paths, people, project names, concepts, tasks, technologies.\n\
Return ONLY a JSON array with no markdown fences. Max 20 entities. Truncate values to 200 chars.\n\n\
Examples of types: ERROR_MSG, COMMAND, FILE, URL, ISSUE, GIT_HASH, \
PERSON, PROJECT, TECHNOLOGY, TASK, DECISION, CONCEPT, EMAIL, DOMAIN";

    // Truncate OCR to keep inference fast (~2000 chars)
    let ocr_truncated = if ocr_text.len() > 2000 {
        &ocr_text[..2000]
    } else {
        ocr_text
    };

    let user_message = format!(
        "App: {app_name}\nWindow: {window_title}\nOCR text:\n{ocr_truncated}"
    );

    let url = format!("{ollama_endpoint}/api/chat");

    let body = serde_json::json!({
        "model": ollama_model,
        "stream": false,
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user",   "content": user_message}
        ],
        "options": {
            "temperature": 0.1,
            "num_predict": 2048
        }
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()?;

    let response = client
        .post(&url)
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await?;

    let status = response.status();
    let text = response.text().await?;

    if !status.is_success() {
        anyhow::bail!("Ollama API error {status}: {text}");
    }

    let resp: serde_json::Value = serde_json::from_str(&text)?;

    // Ollama chat response: {"message": {"content": "..."}}
    let content_text = resp["message"]["content"]
        .as_str()
        .unwrap_or("");

    // DeepSeek-R1 wraps its response in <think>...</think> — strip it
    let content_clean = strip_think_tags(content_text);

    parse_entities(content_clean)
}

/// Strip <think>...</think> blocks produced by DeepSeek-R1 reasoning models.
fn strip_think_tags(s: &str) -> &str {
    // Find the end of the last </think> tag; everything after is the actual answer
    if let Some(end) = s.rfind("</think>") {
        let after = s[end + "</think>".len()..].trim_start();
        if !after.is_empty() {
            return after;
        }
    }
    s
}

/// Remove trailing commas before `]` or `}` — LLMs frequently produce them.
fn fix_trailing_commas(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b',' {
            // Peek ahead past whitespace
            let mut j = i + 1;
            while j < bytes.len()
                && matches!(bytes[j], b' ' | b'\t' | b'\n' | b'\r')
            {
                j += 1;
            }
            if j < bytes.len() && matches!(bytes[j], b']' | b'}') {
                // Skip the trailing comma
                i += 1;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn parse_entities(content: &str) -> Result<Vec<ExtractedEdge>> {
    // Find the JSON array in the response (may be surrounded by whitespace or markdown fences)
    let start = content.find('[').unwrap_or(0);
    let end = content.rfind(']').map(|i| i + 1).unwrap_or(content.len());
    let json_raw = &content[start..end];

    // Strip trailing commas before ] or } (LLMs often produce them)
    let json_str = fix_trailing_commas(json_raw);

    let entities: serde_json::Value = serde_json::from_str(&json_str)
        .map_err(|e| anyhow::anyhow!("Failed to parse entity JSON: {e}\nRaw: {json_raw}"))?;

    let arr = entities
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("Expected JSON array, got: {json_str}"))?;

    let edges = arr
        .iter()
        .filter_map(|item| {
            let entity_type = item["type"].as_str()?;
            let value = item["value"].as_str()?;
            if entity_type.is_empty() || value.is_empty() {
                return None;
            }
            Some(ExtractedEdge::frameless(
                "CONTAINS_ENTITY",
                truncate(entity_type, 100),
                truncate(value, 300),
            ))
        })
        .collect();

    Ok(edges)
}

/// Extract domain from a URL string. Returns None if not parseable.
fn extract_domain(url: &str) -> Option<&str> {
    // Simple domain extraction: find "://" then take up to the next "/"
    let after_scheme = url.find("://").map(|i| &url[i + 3..])?;
    // Strip port and path
    let host_and_port = after_scheme.split('/').next()?;
    let host = host_and_port.split(':').next()?;
    if host.is_empty() {
        None
    } else {
        Some(host)
    }
}

/// Truncate a string to at most `max_bytes` bytes at a char boundary.
fn truncate(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    // Walk back to a char boundary
    let mut idx = max_bytes;
    while idx > 0 && !s.is_char_boundary(idx) {
        idx -= 1;
    }
    &s[..idx]
}
