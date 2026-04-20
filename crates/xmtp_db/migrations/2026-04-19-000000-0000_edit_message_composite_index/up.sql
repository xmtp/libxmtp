-- Replace the single-column index on edited_message_id with a composite
-- index that matches the `ORDER BY edited_at_ns DESC, id ASC` pattern used by
-- `get_latest_edit_by_message_id`. SQLite can satisfy that ordering directly
-- from the index instead of sorting per-query.
DROP INDEX IF EXISTS idx_message_edits_edited_message_id;
CREATE INDEX idx_message_edits_latest
    ON message_edits (edited_message_id, edited_at_ns DESC, id ASC);
