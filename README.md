# Cortex

> **Your AI memory for everything you do on your computer.**
> Ask anything about past sessions, decisions, errors, and fixes — and get a full answer with sources.

<video src="demo/cortex_postgres_demo.mp4" width="100%" controls autoplay muted loop></video>

*Demo: "How did I solve the issue of scaling Postgres last time?" — Cortex surfaces the exact session, failed attempts, final fix, and the config that worked.*

---

## What it does

Cortex runs silently in the background, capturing your screen every few seconds. It extracts structured knowledge from everything you see — commands run, URLs visited, errors hit, files edited, decisions made — and builds a **local Knowledge Graph** on your machine.

Later, you ask a question in plain English. Cortex searches that graph and returns a specific, sourced answer — not a summary of your entire history, but the exact session where you solved that problem.

```
"How did I fix that Postgres scaling issue?"
"What decisions did we make about the API design last Tuesday?"
"Which npm package did I use to solve the auth problem?"
```

---

## How it works

```
macOS screen  →  Apple Vision OCR  →  Structured metadata
                                          │
                               ┌──────────┴──────────┐
                               │   Knowledge Graph    │
                               │  (Nodes + Edges)     │
                               │                      │
                               │  COMMAND ──RAN──▶    │
                               │  URL ──VISITED──▶    │
                               │  FILE ──EDITED──▶    │
                               │  DECISION ──BY──▶    │
                               └──────────┬──────────┘
                                          │
                               DeepSeek-R1 NER (local)
                                          │
                                          ▼
                               Natural language query
                               (Cortex Web UI · :3000)
```

**Everything runs locally.** No data leaves your machine. OCR, NER, and query synthesis all run on-device via [Ollama](https://ollama.com).

---

## Stack

| Layer | Technology |
|---|---|
| Screen capture | Rust + [xcap](https://github.com/nashaofu/xcap) |
| OCR | Apple Vision framework (ObjC FFI) |
| App metadata | ObjC — CGWindowList, NSAppleScript, libproc |
| NER / entity extraction | DeepSeek-R1:7b via Ollama (local) |
| Storage | PostgreSQL 16 + JSONB |
| Knowledge Graph | Custom schema — `kg_nodes`, `kg_edges`, `kg_sessions` |
| Query UI | Rust (Axum 0.7) web server at `:3000` |
| Dashboard | Apache Superset 6 |

---

## Project structure

```
cortex/
├── agent/
│   ├── src/
│   │   ├── main.rs          # Async capture loop (tokio)
│   │   ├── capture.rs       # Screen capture, RGBA→BGRA
│   │   ├── ocr.rs           # Apple Vision OCR (FFI)
│   │   ├── extract.rs       # Rule-based + LLM NER → KG edges
│   │   ├── db.rs            # sqlx, SHA-256 dedup, KG upsert
│   │   ├── config.rs        # TOML config (capture, db, kg)
│   │   └── bin/
│   │       ├── web.rs       # Cortex Web UI (Axum server)
│   │       └── query.rs     # cortex-query CLI
│   └── src-objc/
│       ├── ocr_wrapper.m    # Vision OCR + reading-order sort
│       ├── window_info.m    # Per-monitor frontmost window
│       └── app_metadata.m   # Browser URLs + terminal cwd/cmd
├── database/
│   ├── init.sql             # Base schema + FTS + trgm indexes
│   └── migrate_001_kg.sql   # KG schema migration
├── demo/
│   └── cortex_postgres_demo.mp4
├── Makefile
└── config.toml.example
```

---

## Quick start

### Prerequisites

- macOS (Apple Silicon or Intel)
- Rust — [rustup.rs](https://rustup.rs)
- PostgreSQL 16 — `brew install postgresql@16`
- Ollama — [ollama.com](https://ollama.com) with `ollama pull deepseek-r1:7b`
- **Screen Recording permission** — System Settings → Privacy & Security → Screen Recording

### Run

```bash
# 1. Start Postgres and create the schema
make setup

# 2. Configure
cp config.toml.example config.toml
# set your database URL and Ollama endpoint

# 3. Start the capture agent
make run

# 4. Open the query UI
make web
# → http://localhost:3000
```

### Query from the CLI

```bash
./agent/target/release/cortex-query config.toml \
  "how did I fix the postgres connection issue?"
```

---

## Knowledge Graph schema

```sql
-- Permanent entity nodes (survive OCR TTL)
kg_nodes  (id, node_type TEXT, value TEXT)
  -- node_type: COMMAND, URL, FILE, ERROR_MSG,
  --            TECHNOLOGY, CONCEPT, PROJECT, PERSON, DECISION

-- Edges connect frames → entities
kg_edges  (id, frame_id, relation TEXT, dst_node_id)
  -- relation: RAN, VISITED, WORKING_IN, CONTAINS_ENTITY,
  --           BELONGS_TO_DOMAIN

-- Gap-based session grouping
kg_sessions (id, started_at, ended_at, frame_count)
```

Entity extraction runs in two passes:
1. **Rule-based** — structured metadata → `RAN`, `VISITED`, `WORKING_IN` edges (synchronous, always runs)
2. **DeepSeek-R1 NER** — OCR text → `CONTAINS_ENTITY` edges for semantic entities like errors, decisions, concepts (async, local Ollama)

---

## Cortex Web UI

Natural language query interface at `http://localhost:3000`.

- Type any question about your past sessions
- Cortex queries the KG, executes SQL, and synthesises an answer
- Shows source citations (which sessions the answer came from)
- All inference runs locally — no API keys needed

---

## License

MIT
