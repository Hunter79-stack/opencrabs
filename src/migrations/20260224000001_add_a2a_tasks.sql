-- A2A task persistence: stores task state as JSON for durability across restarts.

CREATE TABLE IF NOT EXISTS a2a_tasks (
    id TEXT PRIMARY KEY NOT NULL,
    context_id TEXT,
    state TEXT NOT NULL DEFAULT 'submitted',  -- submitted, working, completed, failed, canceled
    data TEXT NOT NULL,                        -- Full Task JSON blob
    created_at INTEGER NOT NULL,               -- Unix timestamp
    updated_at INTEGER NOT NULL                -- Unix timestamp
);

CREATE INDEX IF NOT EXISTS idx_a2a_tasks_state ON a2a_tasks(state);
CREATE INDEX IF NOT EXISTS idx_a2a_tasks_updated ON a2a_tasks(updated_at DESC);
