.PHONY: setup build run web psql down reset

# Connection string
DATABASE_URL ?= postgres://screenpipe:screenpipe@localhost:5432/screenpipe

# Homebrew PostgreSQL 16 binary path
PG_BIN ?= /opt/homebrew/opt/postgresql@16/bin

# ── Setup ─────────────────────────────────────────────────────────────────────
setup:
	@echo "==> Starting PostgreSQL (Homebrew)..."
	brew services start postgresql@16
	@echo "==> Waiting for PostgreSQL to be ready..."
	@until $(PG_BIN)/pg_isready >/dev/null 2>&1; do printf '.'; sleep 1; done
	@echo " ready."
	@echo "==> Creating role and database (idempotent)..."
	-$(PG_BIN)/psql postgres -c "CREATE ROLE screenpipe WITH LOGIN PASSWORD 'screenpipe';"
	-$(PG_BIN)/psql postgres -c "CREATE DATABASE screenpipe OWNER screenpipe;"
	$(PG_BIN)/psql -U screenpipe -d screenpipe -f database/init.sql 2>&1 | grep -v "already exists" || true
	$(PG_BIN)/psql -U screenpipe -d screenpipe -f database/migrate_001_kg.sql 2>&1 | grep -v "already exists" || true
	@echo "==> Building agent (release)..."
	cd agent && DATABASE_URL=$(DATABASE_URL) cargo build --release
	@echo "==> Done."
	@[ -f config.toml ] || cp config.toml.example config.toml

# ── Build only ────────────────────────────────────────────────────────────────
build:
	cd agent && DATABASE_URL=$(DATABASE_URL) cargo build --release

# ── Run ───────────────────────────────────────────────────────────────────────
run:
	@[ -f config.toml ] || (echo "ERROR: config.toml not found. Run 'make setup' first."; exit 1)
	RUST_LOG=info ./agent/target/release/screenpipe-agent config.toml

# ── Cortex Query web UI ───────────────────────────────────────────────────────
web:
	@[ -f config.toml ] || (echo "ERROR: config.toml not found. Run 'make setup' first."; exit 1)
	RUST_LOG=info ./agent/target/release/cortex-web config.toml 3000

# ── psql shell ────────────────────────────────────────────────────────────────
psql:
	$(PG_BIN)/psql -U screenpipe -d screenpipe

# ── Stop PostgreSQL ───────────────────────────────────────────────────────────
down:
	brew services stop postgresql@16

# ── Destroy data (drop and recreate empty DB) ─────────────────────────────────
reset:
	-$(PG_BIN)/psql postgres -c "DROP DATABASE screenpipe;"
	-$(PG_BIN)/psql postgres -c "CREATE DATABASE screenpipe OWNER screenpipe;"
	$(PG_BIN)/psql -U screenpipe -d screenpipe -f database/init.sql
	$(PG_BIN)/psql -U screenpipe -d screenpipe -f database/migrate_001_kg.sql
	@echo "==> Data reset."
