/// cortex-query — Natural language query interface over the ScreenPipe knowledge graph.
///
/// Usage:
///   cortex-query config.toml "what did I do to fix the postgres connection error?"
///
/// Two-step LLM pipeline:
///   1. Haiku generates 1-3 PostgreSQL queries from the user's question + KG schema
///   2. Execute queries against the database
///   3. Sonnet synthesizes a natural language answer from the raw results

use anyhow::{Context, Result};
use serde_json::Value;
use sqlx::postgres::PgRow;
use sqlx::{Column, PgPool, Row, TypeInfo};

// Shared modules
#[path = "../config.rs"]
mod config;

async fn connect(url: &str) -> Result<PgPool> {
    Ok(PgPool::connect(url).await?)
}

const KG_SCHEMA: &str = r#"
Tables:
  frames(id BIGINT, captured_at TIMESTAMPTZ, app_name TEXT, window_title TEXT,
         ocr_text TEXT nullable, metadata JSONB, session_id BIGINT nullable)
    -- metadata JSONB examples:
    --   {"type":"terminal","tty":"ttys000","cwd":"/...","foreground_cmd":"cargo build","shell":"zsh"}
    --   {"type":"browser","url":"https://...","tab_title":"...","tab_count":3}
    --   {"type":"app"}

  kg_sessions(id BIGINT, started_at TIMESTAMPTZ, ended_at TIMESTAMPTZ, frame_count INT)

  kg_nodes(id BIGINT, node_type TEXT, value TEXT)
    -- node_type examples: URL, COMMAND, ERROR_MSG, ISSUE, FILE, DIRECTORY,
    --   CONCEPT, TECHNOLOGY, PROJECT, PERSON, TASK, DECISION, GIT_HASH, DOMAIN

  kg_edges(id BIGINT, frame_id BIGINT, src_node_id BIGINT nullable, relation TEXT, dst_node_id BIGINT)
    -- relations: CONTAINS_ENTITY (LLM-extracted), VISITED, RAN, WORKING_IN, BELONGS_TO_DOMAIN

Useful patterns:
  -- Find frames mentioning a concept/error:
  SELECT f.id, f.captured_at, f.app_name, f.metadata->>'foreground_cmd' AS cmd
  FROM frames f JOIN kg_edges e ON e.frame_id = f.id JOIN kg_nodes n ON n.id = e.dst_node_id
  WHERE n.value ILIKE '%connection refused%' ORDER BY f.captured_at DESC LIMIT 20;

  -- Find sessions involving an entity:
  SELECT s.id, s.started_at, s.ended_at, s.frame_count
  FROM kg_sessions s JOIN frames f ON f.session_id = s.id
  JOIN kg_edges e ON e.frame_id = f.id JOIN kg_nodes n ON n.id = e.dst_node_id
  WHERE n.value ILIKE '%postgres%' GROUP BY s.id ORDER BY s.started_at DESC LIMIT 5;

  -- Reconstruct steps in a session:
  SELECT f.captured_at, f.app_name, f.window_title,
         f.metadata->>'foreground_cmd' AS command, f.metadata->>'cwd' AS directory,
         f.metadata->>'url' AS url, f.ocr_text,
         array_agg(n.node_type || ': ' || n.value ORDER BY n.node_type)
             FILTER (WHERE n.id IS NOT NULL) AS entities
  FROM frames f LEFT JOIN kg_edges e ON e.frame_id = f.id
  LEFT JOIN kg_nodes n ON n.id = e.dst_node_id
  WHERE f.session_id = $SESSION_ID
  GROUP BY f.id ORDER BY f.captured_at;
"#;

async fn call_haiku(api_key: &str, system: &str, user: &str) -> Result<String> {
    let body = serde_json::json!({
        "model": "claude-haiku-4-5-20251001",
        "max_tokens": 1024,
        "system": system,
        "messages": [{"role": "user", "content": user}]
    });

    let client = reqwest::Client::new();
    let resp = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await?;

    let status = resp.status();
    let text = resp.text().await?;
    if !status.is_success() {
        anyhow::bail!("Haiku API error {status}: {text}");
    }
    let v: Value = serde_json::from_str(&text)?;
    Ok(v["content"][0]["text"].as_str().unwrap_or("").to_string())
}

async fn call_sonnet(api_key: &str, system: &str, user: &str) -> Result<String> {
    let body = serde_json::json!({
        "model": "claude-sonnet-4-6",
        "max_tokens": 2048,
        "system": system,
        "messages": [{"role": "user", "content": user}]
    });

    let client = reqwest::Client::new();
    let resp = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await?;

    let status = resp.status();
    let text = resp.text().await?;
    if !status.is_success() {
        anyhow::bail!("Sonnet API error {status}: {text}");
    }
    let v: Value = serde_json::from_str(&text)?;
    Ok(v["content"][0]["text"].as_str().unwrap_or("").to_string())
}

/// Execute a SQL query and return results as a JSON array of objects.
async fn execute_query(pool: &PgPool, sql: &str) -> Result<Value> {
    let rows: Vec<PgRow> = sqlx::query(sql).fetch_all(pool).await
        .with_context(|| format!("SQL failed: {sql}"))?;

    let result: Vec<Value> = rows
        .iter()
        .map(|row| {
            let mut obj = serde_json::Map::new();
            for col in row.columns() {
                let name = col.name();
                let type_name = col.type_info().name();
                let val: Value = match type_name {
                    "INT8" | "INT4" | "INT2" => row
                        .try_get::<i64, _>(name)
                        .map(Value::from)
                        .unwrap_or(Value::Null),
                    "BOOL" => row
                        .try_get::<bool, _>(name)
                        .map(Value::from)
                        .unwrap_or(Value::Null),
                    "FLOAT4" | "FLOAT8" => row
                        .try_get::<f64, _>(name)
                        .map(|f| Value::from(f))
                        .unwrap_or(Value::Null),
                    _ => row
                        .try_get::<String, _>(name)
                        .map(Value::from)
                        .unwrap_or(Value::Null),
                };
                obj.insert(name.to_string(), val);
            }
            Value::Object(obj)
        })
        .collect();

    Ok(Value::Array(result))
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let config_path = args.next().unwrap_or_else(|| "config.toml".to_string());
    let question = args.collect::<Vec<_>>().join(" ");

    if question.is_empty() {
        eprintln!("Usage: cortex-query [config.toml] \"your question here\"");
        std::process::exit(1);
    }

    let cfg = config::Config::load(&config_path)
        .with_context(|| format!("Failed to load config from '{config_path}'"))?;

    if cfg.kg.anthropic_api_key.is_empty() {
        anyhow::bail!("anthropic_api_key must be set in config.toml [kg] section");
    }

    let pool = connect(&cfg.database.url).await?;

    // ── Step 1: Haiku generates SQL queries ──────────────────────────────────
    let query_gen_system = format!(
        "You are a query generator for a screen activity knowledge graph.\n\
         {KG_SCHEMA}\n\n\
         Given the user's question, generate 1-3 PostgreSQL queries to retrieve \
         the most relevant sessions, entities, and frame timelines.\n\
         Return ONLY a JSON array of SQL strings. No markdown, no explanation."
    );

    eprintln!("Generating queries...");
    let raw_queries = call_haiku(&cfg.kg.anthropic_api_key, &query_gen_system, &question).await?;

    // Parse the JSON array of SQL strings
    let start = raw_queries.find('[').unwrap_or(0);
    let end = raw_queries.rfind(']').map(|i| i + 1).unwrap_or(raw_queries.len());
    let queries: Vec<String> = serde_json::from_str(&raw_queries[start..end])
        .context("Failed to parse generated SQL queries")?;

    // ── Step 2: Execute each query ────────────────────────────────────────────
    eprintln!("Executing {} queries...", queries.len());
    let mut all_results: Vec<Value> = Vec::new();
    for (i, sql) in queries.iter().enumerate() {
        eprintln!("  Query {}: {}", i + 1, &sql[..sql.len().min(80)]);
        match execute_query(&pool, sql).await {
            Ok(rows) => all_results.push(serde_json::json!({ "query": sql, "rows": rows })),
            Err(e) => {
                eprintln!("  Warning: query {} failed: {e:#}", i + 1);
                all_results.push(serde_json::json!({ "query": sql, "error": e.to_string() }));
            }
        }
    }

    // ── Step 3: Sonnet synthesizes the answer ─────────────────────────────────
    let synth_system = "You are a personal activity assistant. You have access to a user's \
        screen activity knowledge graph. Given query results from their history, \
        answer their question precisely with exact steps, commands, URLs, and \
        error messages as found in the data. Be concise and structured. \
        If results are empty, say so clearly.";

    let results_json = serde_json::to_string_pretty(&all_results)?;
    let synth_user = format!(
        "User question: {question}\n\nQuery results:\n{results_json}"
    );

    eprintln!("Synthesizing answer...\n");
    let answer = call_sonnet(&cfg.kg.anthropic_api_key, synth_system, &synth_user).await?;
    println!("{answer}");

    Ok(())
}
