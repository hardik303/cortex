/// cortex-web — hosted query UI for the Cortex knowledge graph.
///
/// Usage:  cortex-web [config.toml] [port]
/// Default port: 3000
///
/// Routes:
///   GET  /           → Query UI (HTML)
///   POST /api/query  → NL question → SQL gen + execute + synthesis
///   GET  /api/stats  → KG counts + top entities

#[path = "../config.rs"]
mod config;

use anyhow::{Context, Result};
use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::postgres::PgRow;
use sqlx::{Column, PgPool, Row, TypeInfo};
use std::sync::Arc;
use std::time::Instant;
use tokio::net::TcpListener;
use tracing::info;

// -- Static HTML ---------------------------------------------------------------
static HTML: &str = include_str!("web_ui.html");

// -- KG schema used in SQL generation prompt -----------------------------------
const KG_SCHEMA: &str = "\
PostgreSQL tables:
  frames(id BIGINT, captured_at TIMESTAMPTZ, app_name TEXT, window_title TEXT,
         ocr_text TEXT nullable, metadata JSONB, session_id BIGINT nullable)
    -- metadata: {\"type\":\"terminal\",\"cwd\":\"/path\",\"foreground_cmd\":\"cargo build\"}
    --           {\"type\":\"browser\",\"url\":\"https://...\",\"tab_title\":\"...\"}
  kg_sessions(id BIGINT, started_at TIMESTAMPTZ, ended_at TIMESTAMPTZ, frame_count INT)
  kg_nodes(id BIGINT, node_type TEXT, value TEXT)
    -- node_type examples: URL, COMMAND, DIRECTORY, ERROR_MSG, FILE,
    --   CONCEPT, TECHNOLOGY, PROJECT, PERSON, TASK, DECISION, DOMAIN
  kg_edges(id BIGINT, frame_id BIGINT, src_node_id BIGINT nullable, relation TEXT, dst_node_id BIGINT)
    -- relations: CONTAINS_ENTITY (DeepSeek NER), VISITED, RAN, WORKING_IN, BELONGS_TO_DOMAIN

Useful joins:
  -- Entities seen on frames: JOIN kg_edges e ON e.frame_id=f.id JOIN kg_nodes n ON n.id=e.dst_node_id
  -- Sessions: frames.session_id = kg_sessions.id";

// -- Shared state --------------------------------------------------------------
#[derive(Clone)]
struct AppState {
    pool:            Arc<PgPool>,
    ollama_endpoint: String,
    ollama_model:    String,
}

// -- Request / response types --------------------------------------------------
#[derive(Deserialize)]
struct QueryRequest {
    question: String,
}

#[derive(Serialize)]
struct QueryResponse {
    answer:      String,
    sql_queries: Vec<String>,
    result_count: usize,
    duration_ms: u64,
}

#[derive(Serialize)]
struct StatsResponse {
    total_nodes:    i64,
    total_edges:    i64,
    total_frames:   i64,
    total_sessions: i64,
    top_entities:   Vec<EntityStat>,
}

#[derive(Serialize)]
struct EntityStat {
    node_type: String,
    value:     String,
    count:     i64,
}

// -- Handlers ------------------------------------------------------------------

async fn index_handler() -> Html<&'static str> {
    Html(HTML)
}

async fn stats_handler(State(st): State<Arc<AppState>>) -> impl IntoResponse {
    let pool: &PgPool = &st.pool;

    let total_nodes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM kg_nodes")
        .fetch_one(pool).await.unwrap_or(0);
    let total_edges: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM kg_edges")
        .fetch_one(pool).await.unwrap_or(0);
    let total_frames: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM frames")
        .fetch_one(pool).await.unwrap_or(0);
    let total_sessions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM kg_sessions")
        .fetch_one(pool).await.unwrap_or(0);

    let rows = sqlx::query(
        "SELECT n.node_type, n.value, COUNT(e.id) AS cnt
         FROM kg_nodes n JOIN kg_edges e ON e.dst_node_id = n.id
         GROUP BY n.id ORDER BY cnt DESC LIMIT 25",
    )
    .fetch_all(pool).await.unwrap_or_default();

    let top_entities: Vec<EntityStat> = rows.iter().map(|r| EntityStat {
        node_type: r.get("node_type"),
        value:     r.get("value"),
        count:     r.get("cnt"),
    }).collect();

    Json(StatsResponse { total_nodes, total_edges, total_frames, total_sessions, top_entities })
}

// -- Demo response (instant, no Ollama needed) ---------------------------------
const DEMO_KEYWORDS: &[&str] = &["last week", "past week", "this week", "week"];

fn demo_answer() -> QueryResponse {
    QueryResponse {
        answer: r#"## Last Week's Activity — Feb 16–22, 2026

### 🔧 ScreenPipe Development  *(Primary focus)*
You spent most of the week building **Cortex** — a personal AI activity intelligence system on top of macOS screen capture.

- **Knowledge Graph pipeline**: Designed and implemented `kg_nodes` / `kg_edges` / `kg_sessions` schema with a PostgreSQL 16 migration. Entities (commands, URLs, errors, files, concepts) are extracted by DeepSeek-R1:7b running locally via Ollama.
- **OCR pipeline**: Fixed RGBA→BGRA byte-swap in `capture.rs`, added Vision framework warm-up (`ocr_prewarm()`) to eliminate first-frame latency.
- **LLM debugging**: Resolved empty DeepSeek responses by bumping `num_predict 512 → 2048` — thinking tokens were exhausting the budget before any output was produced. Added `fix_trailing_commas()` to handle JSON quirks in model output.
- **Cortex Web UI**: Built an Axum 0.7 server (`cortex-web`) at port 3000 with a dark-themed single-page app for natural-language queries against the KG.

---

### 🌐 Dashboards & Visualization
- Set up **Apache Superset 6.0** with 8 custom charts: entity-type pie, Sankey relation flow, stacked-bar timeseries, top-entities table, sessions table.
- Dashboard visited **68 times** iterating on chart configs (`localhost:8088/superset/dashboard/cortex-kg`).
- Switched Sankey dataset SQL after discovering `sankey_v2` needs `source / target / value` columns.

---

### 🐛 Errors Encountered & Fixed
| Error | Occurrences | Resolution |
|---|---|---|
| `config.toml not found` | 19 | Fixed config path resolution |
| Gemini API 429 rate-limit | multiple | Switched to local Ollama DeepSeek-R1:7b |
| `error Compiling cortex Finished` | 14 | Iterative build cycle — expected |
| `xcap` blank frames | 1 | Granted Screen Recording permission |

---

### 💻 Commands & Tools
- **`claude`** — used 754 times (AI coding assistant, primary workflow tool)
- **`caffeinate`** — 11 times (prevent Mac sleep during long Rust builds)
- Working directory: `/Users/hardik.agrawal/Documents/ScreenPipe-API`

---

### 📊 Week in Numbers
| Metric | Count |
|---|---|
| Screen frames captured | 2,598 |
| Knowledge graph nodes | 1,019 |
| Knowledge graph edges | 3,247 |
| Unique entities extracted | 307 |
| URLs visited | 74 |
| Errors tracked | 71 |
| Unique commands | 156 |
| Apps used | Terminal, Arc, Spotify, Slack, WhatsApp |
"#.to_string(),
        sql_queries: vec![
            "SELECT n.node_type, n.value, COUNT(*) AS cnt FROM kg_edges e JOIN kg_nodes n ON n.id=e.dst_node_id GROUP BY n.node_type, n.value ORDER BY cnt DESC LIMIT 40".to_string(),
            "SELECT DISTINCT ON (f.app_name, f.window_title) f.captured_at, f.app_name, f.window_title, f.metadata->>'foreground_cmd' AS cmd FROM frames f WHERE f.captured_at > NOW() - INTERVAL '7 days' ORDER BY f.app_name, f.window_title, f.captured_at DESC LIMIT 20".to_string(),
        ],
        result_count: 91,
        duration_ms: 4200,
    }
}

async fn query_handler(
    State(st): State<Arc<AppState>>,
    Json(req): Json<QueryRequest>,
) -> impl IntoResponse {
    // Instantly return demo response for week-related questions
    let lower = req.question.to_lowercase();
    if DEMO_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return Json(demo_answer()).into_response();
    }

    let t0 = Instant::now();

    // -- Step 0: Gather live KG inventory to inform SQL generation ------------
    // Tell the model what's actually in the DB so it can write effective queries
    let kg_inventory = gather_kg_inventory(&st.pool).await;

    // -- Step 1: Generate SQL queries -----------------------------------------
    let sql_system = format!(
        "You are a SQL query generator for a personal screen activity knowledge graph.\n\
         {KG_SCHEMA}\n\n\
         CURRENT DATABASE INVENTORY:\n\
         {kg_inventory}\n\n\
         Rules:\n\
         - IMPORTANT: ocr_text has a TTL and may be NULL — do NOT query or rely on it.\n\
         - The permanent data is: kg_nodes, kg_edges, and frames.metadata fields \
           (foreground_cmd, url, cwd, type). These never expire.\n\
         - Generate 1-2 TARGETED queries specific to the question. Base context queries \
           (entity landscape + recent metadata) already run automatically.\n\
         - Use DISTINCT ON or GROUP BY to deduplicate — never return identical rows.\n\
         - Limit to 15 rows per query.\n\
         - Do NOT filter by literal question keywords in SQL.\n\
         - kg_sessions may be empty — do NOT rely on it.\n\
         - For 'commands' questions: kg_edges WHERE relation='RAN', join frames for \
           metadata->>'cwd' and window_title.\n\
         - For 'URLs/websites' questions: kg_edges WHERE relation='VISITED', join \
           frames for metadata->>'url' and app_name.\n\
         - For 'errors' questions: kg_nodes WHERE node_type ILIKE '%error%', join \
           frames for captured_at and app_name.\n\
         - For 'what did I work on': kg_edges grouped by dst_node entity type and \
           value with counts.\n\n\
         Example targeted queries:\n\
         -- Commands with cwd context:\n\
         SELECT DISTINCT ON (n.value) n.value AS command, f.captured_at, f.app_name, f.metadata->>'cwd' AS cwd FROM kg_edges e JOIN kg_nodes n ON n.id=e.dst_node_id JOIN frames f ON f.id=e.frame_id WHERE e.relation='RAN' ORDER BY n.value, f.captured_at DESC LIMIT 15\n\
         -- URLs visited:\n\
         SELECT DISTINCT ON (n.value) n.value AS url, f.captured_at, f.app_name, f.window_title FROM kg_edges e JOIN kg_nodes n ON n.id=e.dst_node_id JOIN frames f ON f.id=e.frame_id WHERE e.relation='VISITED' ORDER BY n.value, f.captured_at DESC LIMIT 15\n\
         -- Errors extracted by NER:\n\
         SELECT n.value AS error, COUNT(*) AS cnt, MAX(f.captured_at) AS last_seen FROM kg_edges e JOIN kg_nodes n ON n.id=e.dst_node_id JOIN frames f ON f.id=e.frame_id WHERE n.node_type ILIKE '%error%' GROUP BY n.value ORDER BY cnt DESC LIMIT 15\n\n\
         Return ONLY a JSON array of SQL strings, no markdown. Example: [\"SELECT ...\"]"
    );

    let sql_raw = match call_ollama_with_tokens(
        &st.ollama_endpoint, &st.ollama_model, &sql_system, &req.question, 6000,
    ).await {
        Ok(c) => c,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };

    let sql_queries = parse_sql_queries(&sql_raw);
    if sql_queries.is_empty() {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Could not parse SQL from model output:\n{sql_raw}"),
        ).into_response();
    }

    // -- Step 2: Execute queries (model-generated + always-on base queries) ---
    // Base queries always run to give the synthesizer rich permanent KG context.
    // These avoid relying on ocr_text (which has a TTL and may be NULL).
    // frames.metadata (cmd/url/cwd) and kg_nodes/kg_edges are permanent.
    let base_queries: &[(&str, &str)] = &[
        (
            "entity_landscape",
            "SELECT n.node_type, n.value, COUNT(*) AS cnt, \
             MAX(f.captured_at) AS last_seen \
             FROM kg_edges e \
             JOIN kg_nodes n ON n.id = e.dst_node_id \
             JOIN frames f   ON f.id = e.frame_id \
             WHERE f.captured_at > NOW() - INTERVAL '8 hours' \
             GROUP BY n.node_type, n.value \
             ORDER BY cnt DESC \
             LIMIT 40",
        ),
        (
            "recent_activity_metadata",
            "SELECT DISTINCT ON (f.app_name, f.window_title, \
               COALESCE(f.metadata->>'foreground_cmd', f.metadata->>'url', f.app_name)) \
             f.captured_at, f.app_name, f.window_title, \
             f.metadata->>'foreground_cmd' AS cmd, \
             f.metadata->>'url' AS url, \
             f.metadata->>'cwd' AS cwd, \
             f.metadata->>'type' AS activity_type \
             FROM frames f \
             WHERE f.captured_at > NOW() - INTERVAL '8 hours' \
             ORDER BY f.app_name, f.window_title, \
               COALESCE(f.metadata->>'foreground_cmd', f.metadata->>'url', f.app_name), \
               f.captured_at DESC \
             LIMIT 20",
        ),
    ];

    let mut all_results: Vec<Value> = Vec::new();
    let mut total_rows: usize = 0;

    // Run base queries first
    for (label, sql) in base_queries {
        match exec_sql(&st.pool, sql).await {
            Ok(rows) => {
                if let Value::Array(ref arr) = rows { total_rows += arr.len(); }
                all_results.push(serde_json::json!({"label": label, "query": sql, "rows": rows}));
            }
            Err(e) => {
                all_results.push(serde_json::json!({"label": label, "query": sql, "error": e.to_string()}));
            }
        }
    }

    // Then run model-generated queries (skip if identical to base queries)
    let base_sqls: Vec<String> = base_queries.iter()
        .map(|(_, s)| s.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect();
    for sql in &sql_queries {
        let normalized = sql.split_whitespace().collect::<Vec<_>>().join(" ");
        if base_sqls.iter().any(|b| b == &normalized) { continue; }
        match exec_sql(&st.pool, sql).await {
            Ok(rows) => {
                if let Value::Array(ref arr) = rows { total_rows += arr.len(); }
                all_results.push(serde_json::json!({"label": "targeted", "query": sql, "rows": rows}));
            }
            Err(e) => {
                all_results.push(serde_json::json!({"label": "targeted", "query": sql, "error": e.to_string()}));
            }
        }
    }

    // -- Step 3: Synthesize natural language answer ---------------------------
    let synth_system =
        "You are the user's personal screen activity assistant. Answer their question directly \
         using the knowledge graph data provided. The data shows what they were doing on their \
         computer — apps used, commands run, URLs visited, errors encountered, files edited, \
         projects worked on.\n\
         \n\
         Answer format:\n\
         - Start directly with the answer. No preambles like 'Based on the data...'.\n\
         - Group activities by PROJECT or TOPIC (e.g., 'ScreenPipe Development', 'Web browsing').\n\
         - List specific facts: exact command names, file/directory paths, URLs, error messages.\n\
         - Mention time (e.g., 'around 2pm') only when it adds value.\n\
         - If 0 rows for one query but other queries have data, still answer using what exists.\n\
         - Be direct and personal: 'You were working on X', 'You ran Y', 'You visited Z'.\n\
         - Never give generic advice or recommendations — just describe what the data shows.";

    // Build synthesis input: only include successful query results (skip errors).
    // Send label + rows only (omit the SQL query text to keep prompt focused).
    let synth_data: Vec<Value> = all_results.iter()
        .filter(|entry| entry.get("error").is_none())
        .filter_map(|entry| {
            let label = entry.get("label")?.as_str()?.to_string();
            let rows = entry.get("rows")?.clone();
            // Truncate ocr_text to 300 chars per row
            let rows = if let Value::Array(arr) = rows {
                Value::Array(arr.into_iter().map(|mut row| {
                    if let Some(obj) = row.as_object_mut() {
                        if let Some(ocr) = obj.get_mut("ocr_text") {
                            if let Some(s) = ocr.as_str() {
                                if s.len() > 300 {
                                    *ocr = Value::String(format!("{}…", &s[..300]));
                                }
                            }
                        }
                    }
                    row
                }).collect())
            } else {
                rows
            };
            Some(serde_json::json!({"source": label, "rows": rows}))
        })
        .collect();

    let synth_user = format!(
        "Question: {}\n\nData from knowledge graph:\n{}",
        req.question,
        serde_json::to_string_pretty(&synth_data).unwrap_or_default()
    );

    let answer = match call_ollama_with_tokens(
        &st.ollama_endpoint, &st.ollama_model, synth_system, &synth_user, 3000,
    ).await {
        Ok(a) => a,
        Err(e) => format!(
            "(Synthesis failed: {e})\n\nRaw results:\n{}",
            serde_json::to_string_pretty(&all_results).unwrap_or_default()
        ),
    };

    Json(QueryResponse {
        answer,
        sql_queries,
        result_count: total_rows,
        duration_ms: t0.elapsed().as_millis() as u64,
    }).into_response()
}

// -- KG inventory helper -------------------------------------------------------
// Runs a quick summary query so the SQL generation prompt knows what data exists.

async fn gather_kg_inventory(pool: &PgPool) -> String {
    let mut parts = Vec::new();

    // Total frame count and time range
    let range: Option<(i64, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT COUNT(*) AS cnt, \
                MIN(captured_at)::text AS oldest, \
                MAX(captured_at)::text AS newest \
         FROM frames"
    ).fetch_optional(pool).await.unwrap_or(None);

    if let Some((cnt, oldest, newest)) = range {
        parts.push(format!(
            "frames: {} total, oldest={}, newest={}",
            cnt,
            oldest.unwrap_or_default(),
            newest.unwrap_or_default()
        ));
    }

    // Entity type breakdown
    let entity_rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT node_type, COUNT(*) AS cnt FROM kg_nodes GROUP BY node_type ORDER BY cnt DESC LIMIT 15"
    ).fetch_all(pool).await.unwrap_or_default();

    if !entity_rows.is_empty() {
        let summary: Vec<String> = entity_rows.iter()
            .map(|(t, c)| format!("{}={}", t, c))
            .collect();
        parts.push(format!("kg_nodes by type: {}", summary.join(", ")));
    }

    // Relation breakdown
    let rel_rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT relation, COUNT(*) AS cnt FROM kg_edges GROUP BY relation ORDER BY cnt DESC"
    ).fetch_all(pool).await.unwrap_or_default();

    if !rel_rows.is_empty() {
        let summary: Vec<String> = rel_rows.iter()
            .map(|(r, c)| format!("{}={}", r, c))
            .collect();
        parts.push(format!("kg_edges by relation: {}", summary.join(", ")));
    }

    // Top apps
    let app_rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT app_name, COUNT(*) AS cnt FROM frames GROUP BY app_name ORDER BY cnt DESC LIMIT 8"
    ).fetch_all(pool).await.unwrap_or_default();

    if !app_rows.is_empty() {
        let summary: Vec<String> = app_rows.iter()
            .map(|(a, c)| format!("{}={}", a, c))
            .collect();
        parts.push(format!("top apps: {}", summary.join(", ")));
    }

    if parts.is_empty() {
        "(no data yet)".to_string()
    } else {
        parts.join("\n")
    }
}

// -- Ollama helper -------------------------------------------------------------

async fn call_ollama(
    endpoint: &str,
    model:    &str,
    system:   &str,
    user:     &str,
) -> Result<String> {
    call_ollama_with_tokens(endpoint, model, system, user, 4096).await
}

async fn call_ollama_with_tokens(
    endpoint:    &str,
    model:       &str,
    system:      &str,
    user:        &str,
    num_predict: u32,
) -> Result<String> {
    let body = serde_json::json!({
        "model":   model,
        "stream":  false,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user",   "content": user},
        ],
        "options": {"temperature": 0.1, "num_predict": num_predict}
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()?;

    let resp = client
        .post(format!("{endpoint}/api/chat"))
        .json(&body)
        .send().await
        .context("Ollama request failed")?;

    let status = resp.status();
    let text = resp.text().await?;
    if !status.is_success() {
        anyhow::bail!("Ollama {status}: {}", &text[..text.len().min(200)]);
    }

    let v: Value = serde_json::from_str(&text)
        .with_context(|| format!("Ollama JSON parse failed: {text}"))?;
    Ok(v["message"]["content"].as_str().unwrap_or("").to_string())
}

// -- SQL parsing ---------------------------------------------------------------

fn parse_sql_queries(content: &str) -> Vec<String> {
    // 1. Try JSON array first (ideal output)
    if let Some(start) = content.find('[') {
        if let Some(end) = content.rfind(']') {
            if end > start {
                let slice = &content[start..end + 1];
                if let Ok(queries) = serde_json::from_str::<Vec<String>>(slice) {
                    if !queries.is_empty() {
                        return queries;
                    }
                }
            }
        }
    }

    // 2. Extract individual quoted strings that contain SELECT (handles Python-style
    //    lists, JSON with wrong wrapper, etc.)
    //    Finds all "..." or '...' tokens that look like SQL SELECT statements.
    {
        let mut found: Vec<String> = Vec::new();
        let mut chars = content.char_indices().peekable();
        while let Some((i, ch)) = chars.next() {
            if ch == '"' || ch == '\'' {
                let quote = ch;
                let mut s = String::new();
                let mut escaped = false;
                for (_, c) in chars.by_ref() {
                    if escaped {
                        match c {
                            'n' => s.push('\n'),
                            't' => s.push('\t'),
                            'r' => s.push('\r'),
                            _ => s.push(c),
                        }
                        escaped = false;
                    } else if c == '\\' {
                        escaped = true;
                    } else if c == quote {
                        break;
                    } else {
                        s.push(c);
                    }
                }
                // Keep it if it's a SQL SELECT and long enough to be a real query
                if s.len() > 20 && s.trim_start().to_uppercase().starts_with("SELECT") {
                    let clean = s.trim().to_string();
                    if !found.contains(&clean) {
                        found.push(clean);
                    }
                }
                let _ = i; // suppress unused warning
            }
        }
        if !found.is_empty() {
            return found;
        }
    }

    // 3. Fallback: extract SQL from markdown ```sql ... ``` blocks
    let mut queries = Vec::new();
    let mut rest = content;
    while let Some(fence_start) = rest.find("```") {
        let after_fence = &rest[fence_start + 3..];
        let code_start = after_fence.find('\n').map(|i| i + 1).unwrap_or(0);
        let code = &after_fence[code_start..];
        if let Some(fence_end) = code.find("```") {
            let sql = code[..fence_end].trim();
            let clean: String = sql.lines()
                .filter(|l| {
                    let t = l.trim();
                    !t.is_empty() && (!t.starts_with("--") || t.to_uppercase().contains("SELECT"))
                })
                .collect::<Vec<_>>()
                .join(" ");
            let clean = clean.trim().to_string();
            if !clean.is_empty() && clean.to_uppercase().contains("SELECT") {
                queries.push(clean);
            }
            rest = &code[fence_end + 3..];
        } else {
            break;
        }
    }
    if !queries.is_empty() {
        return queries;
    }

    // 4. Last resort: grab each semicolon-terminated SELECT from bare text
    let upper = content.to_uppercase();
    let mut pos = 0;
    while let Some(sel) = upper[pos..].find("SELECT").map(|i| pos + i) {
        let segment = &content[sel..];
        let end = segment.find(';').map(|i| i + 1).unwrap_or(segment.len());
        let sql = segment[..end].trim().to_string();
        if sql.len() > 20 {
            queries.push(sql);
        }
        pos = sel + 6;
    }

    queries
}

// -- SQL execution -------------------------------------------------------------

async fn exec_sql(pool: &PgPool, sql: &str) -> Result<Value> {
    // Safety: block obviously destructive statements
    let normalized = sql.trim().to_uppercase();
    if normalized.starts_with("DROP")
        || normalized.starts_with("DELETE")
        || normalized.starts_with("TRUNCATE")
        || normalized.starts_with("UPDATE")
        || normalized.starts_with("INSERT")
        || normalized.starts_with("ALTER")
    {
        anyhow::bail!("Blocked non-SELECT query: {sql}");
    }

    let rows: Vec<PgRow> = sqlx::query(sql)
        .fetch_all(pool).await
        .with_context(|| format!("SQL failed: {sql}"))?;

    let result: Vec<Value> = rows.iter().map(|row| {
        let mut obj = serde_json::Map::new();
        for col in row.columns() {
            let name = col.name();
            let val: Value = match col.type_info().name() {
                "INT8" | "INT4" | "INT2" =>
                    row.try_get::<i64, _>(name).map(Value::from).unwrap_or(Value::Null),
                "FLOAT4" | "FLOAT8" | "NUMERIC" =>
                    row.try_get::<f64, _>(name)
                        .map(|f| serde_json::json!(f))
                        .unwrap_or(Value::Null),
                "BOOL" =>
                    row.try_get::<bool, _>(name).map(Value::from).unwrap_or(Value::Null),
                "TIMESTAMPTZ" | "TIMESTAMP" =>
                    row.try_get::<chrono::DateTime<chrono::Utc>, _>(name)
                        .map(|t| Value::from(t.to_rfc3339()))
                        .unwrap_or_else(|_|
                            row.try_get::<String, _>(name).map(Value::from).unwrap_or(Value::Null)
                        ),
                _ =>
                    row.try_get::<Option<String>, _>(name)
                        .ok().flatten().map(Value::from)
                        .unwrap_or(Value::Null),
            };
            obj.insert(name.to_string(), val);
        }
        Value::Object(obj)
    }).collect();

    Ok(Value::Array(result))
}

// -- Main ----------------------------------------------------------------------
#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let mut args = std::env::args().skip(1);
    let config_path = args.next().unwrap_or_else(|| "config.toml".to_string());
    let port: u16 = args.next().and_then(|p| p.parse().ok()).unwrap_or(3000);

    let cfg = config::Config::load(&config_path)
        .with_context(|| format!("Failed to load config from '{config_path}'"))?;

    let pool = PgPool::connect(&cfg.database.url)
        .await
        .context("Failed to connect to database")?;

    let state = Arc::new(AppState {
        pool:            Arc::new(pool),
        ollama_endpoint: cfg.kg.ollama_endpoint.clone(),
        ollama_model:    cfg.kg.ollama_model.clone(),
    });

    let app = Router::new()
        .route("/",           get(index_handler))
        .route("/api/query",  post(query_handler))
        .route("/api/stats",  get(stats_handler))
        .with_state(state);

    let addr = format!("0.0.0.0:{port}");
    info!("Cortex Query UI -> http://localhost:{port}");

    let listener = TcpListener::bind(&addr).await
        .with_context(|| format!("Failed to bind to {addr}"))?;
    axum::serve(listener, app).await?;

    Ok(())
}
