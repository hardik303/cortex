-- ScreenPipe schema
-- Enable trigram extension for partial/URL text search
CREATE EXTENSION IF NOT EXISTS pg_trgm;

CREATE TABLE frames (
    id              BIGSERIAL PRIMARY KEY,
    captured_at     TIMESTAMPTZ NOT NULL,
    app_name        TEXT NOT NULL DEFAULT '',
    window_title    TEXT NOT NULL DEFAULT '',
    ocr_text        TEXT NOT NULL,
    monitor_id      BIGINT NOT NULL,
    ocr_text_hash   CHAR(64) NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Full-text search on OCR content
CREATE INDEX idx_frames_ocr_fts
    ON frames USING GIN (to_tsvector('english', ocr_text));

-- Trigram index for partial/URL substring search
CREATE INDEX idx_frames_ocr_trgm
    ON frames USING GIN (ocr_text gin_trgm_ops);

-- Time-range queries (most-recent first)
CREATE INDEX idx_frames_captured_at
    ON frames (captured_at DESC);

-- Dedup check: covering index enables index-only scan
CREATE INDEX idx_frames_dedup
    ON frames (monitor_id, captured_at DESC)
    INCLUDE (ocr_text_hash);
