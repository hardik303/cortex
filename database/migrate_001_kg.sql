-- Migration 001: Temporal Knowledge Graph + OCR TTL
-- Apply with:  psql -U cortex -d cortex -f database/migrate_001_kg.sql

-- 1. Make ocr_text nullable (required for TTL expiry)
ALTER TABLE frames ALTER COLUMN ocr_text DROP NOT NULL;

-- 2. Sessions table
CREATE TABLE IF NOT EXISTS kg_sessions (
    id          BIGSERIAL PRIMARY KEY,
    started_at  TIMESTAMPTZ NOT NULL,
    ended_at    TIMESTAMPTZ NOT NULL,
    frame_count INT NOT NULL DEFAULT 0,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 3. FK column on frames
ALTER TABLE frames ADD COLUMN IF NOT EXISTS session_id BIGINT REFERENCES kg_sessions(id) ON DELETE SET NULL;

-- 4. Rebuild FTS/trgm indexes as partial (skip NULL rows)
DROP INDEX IF EXISTS idx_frames_ocr_fts;
DROP INDEX IF EXISTS idx_frames_ocr_trgm;
CREATE INDEX idx_frames_ocr_fts
    ON frames USING GIN (to_tsvector('english', ocr_text))
    WHERE ocr_text IS NOT NULL;
CREATE INDEX idx_frames_ocr_trgm
    ON frames USING GIN (ocr_text gin_trgm_ops)
    WHERE ocr_text IS NOT NULL;

-- 5. New frame indexes
CREATE INDEX IF NOT EXISTS idx_frames_unassigned
    ON frames (captured_at ASC)
    WHERE session_id IS NULL;
CREATE INDEX IF NOT EXISTS idx_frames_session_id
    ON frames (session_id, captured_at ASC);

-- 6. KG nodes — open string node_type (no enum)
CREATE TABLE IF NOT EXISTS kg_nodes (
    id         BIGSERIAL PRIMARY KEY,
    node_type  TEXT NOT NULL,
    value      TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uq_kg_nodes UNIQUE (node_type, value)
);
CREATE INDEX IF NOT EXISTS idx_kg_nodes_type_value ON kg_nodes (node_type, value);
CREATE INDEX IF NOT EXISTS idx_kg_nodes_value_trgm ON kg_nodes USING GIN (value gin_trgm_ops);

-- 7. KG edges — append-only occurrence log
CREATE TABLE IF NOT EXISTS kg_edges (
    id          BIGSERIAL PRIMARY KEY,
    frame_id    BIGINT NOT NULL REFERENCES frames(id) ON DELETE CASCADE,
    src_node_id BIGINT          REFERENCES kg_nodes(id) ON DELETE CASCADE,
    relation    TEXT NOT NULL,
    dst_node_id BIGINT NOT NULL REFERENCES kg_nodes(id) ON DELETE CASCADE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_kg_edges_frame_id ON kg_edges (frame_id);
CREATE INDEX IF NOT EXISTS idx_kg_edges_dst      ON kg_edges (dst_node_id, frame_id);
CREATE INDEX IF NOT EXISTS idx_kg_edges_src      ON kg_edges (src_node_id, frame_id) WHERE src_node_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_kg_edges_relation ON kg_edges (relation);

-- 8. Sessions index
CREATE INDEX IF NOT EXISTS idx_kg_sessions_time ON kg_sessions (started_at DESC);
