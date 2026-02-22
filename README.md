# cortex

A local-first macOS screen activity recorder. Continuously captures all monitors, runs Apple Vision OCR to extract text, and enriches every frame with app-specific metadata — browser URLs, terminal commands, working directories. Everything is stored in PostgreSQL and visualized in a live Superset dashboard.

## How it works

```
Screen capture (xcap)
      │
      ▼
Per-monitor window detection (CGWindowList)
      │
      ▼
App metadata extraction (AppleScript + libproc)
  • Browsers  → URL, tab title, tab count
  • Terminals → TTY, cwd, foreground command, shell
      │
      ▼
Apple Vision OCR (VNRecognizeTextRequest, accurate mode)
      │
      ▼
PostgreSQL  ←─ SHA-256 dedup (skips unchanged screens)
      │
      ▼
Superset dashboard (live, auto-refresh)
```

## Stack

| Layer | Technology |
|---|---|
| Capture | Rust + [xcap](https://github.com/nashaofu/xcap) |
| OCR | Apple Vision framework (ObjC FFI) |
| Window & app metadata | ObjC — CGWindowList, NSAppleScript, libproc |
| Storage | PostgreSQL 16 + JSONB metadata column |
| Dashboard | Apache Superset 6 |

## Project structure

```
cortex/
├── agent/                     # Native macOS Rust binary
│   ├── src/
│   │   ├── main.rs            # Async capture loop (tokio)
│   │   ├── capture.rs         # Screen capture, RGBA→BGRA
│   │   ├── ocr.rs             # FFI bridge to Vision OCR
│   │   ├── window_info.rs     # FFI bridge to CGWindowList
│   │   ├── app_metadata.rs    # FFI bridge to app metadata
│   │   ├── db.rs              # sqlx + SHA-256 dedup
│   │   └── config.rs          # TOML config
│   ├── src-objc/
│   │   ├── ocr_wrapper.m      # Vision OCR + reading-order sort
│   │   ├── window_info.m      # Per-monitor frontmost window
│   │   └── app_metadata.m     # Browser URLs + terminal metadata
│   ├── build.rs               # Compiles ObjC, links frameworks
│   └── Cargo.toml
├── database/
│   └── init.sql               # Schema, FTS, trgm, dedup indexes
├── docker-compose.yml         # PostgreSQL 16
├── Makefile                   # setup / build / run / psql
├── build_dashboard.py         # Creates Superset charts & dashboard
├── superset_config.py         # Superset config
└── config.toml.example        # Example agent config
```

## Setup

### Prerequisites

- macOS (Apple Silicon or Intel)
- Rust toolchain — [rustup.rs](https://rustup.rs)
- PostgreSQL 16 — `brew install postgresql@16` or Docker
- Python 3.11+ (for Superset)
- **Screen Recording permission** — System Settings → Privacy & Security → Screen Recording

### Run

```bash
# 1. Start PostgreSQL and build the agent
make setup

# 2. Configure
cp config.toml.example config.toml
# edit config.toml — set database.url if needed

# 3. Run the agent
make run

# 4. Query the database
make psql
```

```sql
SELECT captured_at, app_name, metadata, length(ocr_text)
FROM frames
ORDER BY captured_at DESC
LIMIT 10;
```

### Superset dashboard

```bash
python3 -m venv superset-venv
source superset-venv/bin/activate
pip install apache-superset

export SUPERSET_CONFIG_PATH=$(pwd)/superset_config.py
superset db upgrade
superset fab create-admin
superset init

# Start
superset run -p 8088 --with-threads

# Build charts & dashboard
python3 build_dashboard.py
```

Open [http://localhost:8088](http://localhost:8088).

## Metadata schema

Every frame stores a `metadata JSONB` column:

```json
// Terminal
{"type": "terminal", "tty": "ttys003", "cwd": "/Users/you/project", "foreground_cmd": "cargo build", "shell": "zsh"}

// Browser
{"type": "browser", "url": "https://github.com", "tab_title": "GitHub", "tab_count": 12}

// Other app
{"type": "app"}
```

## Configuration

```toml
[capture]
interval_secs = 5.0   # capture frequency

[database]
url = "postgresql://screenpipe:screenpipe@localhost:5432/screenpipe"
```

## License

MIT
