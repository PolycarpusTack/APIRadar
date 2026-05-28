-- Add async generation tracking columns to release_note.
-- generation_status: pending | running | completed | failed (NULL = not yet generated)
-- generation_error:  last error message when generation_status = 'failed'
ALTER TABLE release_note ADD COLUMN generation_status TEXT;
ALTER TABLE release_note ADD COLUMN generation_error  TEXT;
