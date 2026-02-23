# Cortex

Your computer has seen everything you've ever worked on. Cortex makes it searchable.

It runs silently in the background, builds a knowledge graph of your activity, and lets you ask questions in plain English — getting back specific, sourced answers from your own history.

<br/>

![Cortex Demo](demo/cortex_demo.gif)

*"How did I solve the issue of scaling Postgres last time?" — Cortex surfaces the exact session: what failed, what worked, and the config that fixed it.*

<br/>

## What you can ask

```
"How did I solve the issue of scaling Postgres last time?"
"What was the API design decision we made in January?"
"Which library did I use to fix the auth bug?"
"What errors keep showing up in the build pipeline?"
```

## Features

- **Fully local** — screen capture, OCR, and AI inference all run on your machine. Nothing leaves.
- **Knowledge Graph** — entities (commands, URLs, errors, files, decisions) extracted and linked across sessions
- **Temporal memory** — knows *when* something happened and can trace sessions across days
- **Natural language query** — ask in plain English, get a sourced answer with citations
- **OCR TTL** — raw screen text expires automatically; structured knowledge is permanent

## Stack

| | |
|---|---|
| Capture | Rust + xcap |
| OCR | Apple Vision (on-device) |
| NER | DeepSeek-R1:7b via Ollama |
| Storage | PostgreSQL 16 |
| Query UI | Axum web server |

## Quickstart

```bash
# Prerequisites: Rust, PostgreSQL 16, Ollama (ollama pull deepseek-r1:7b)
# Grant Screen Recording in System Settings → Privacy & Security

make setup          # create database schema
cp config.toml.example config.toml
make run            # start capture agent
make web            # open query UI at localhost:3000
```

## License

MIT
