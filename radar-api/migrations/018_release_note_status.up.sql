-- H-4: Release-note state workflow (draft → reviewed → published → superseded)
ALTER TABLE release_note ADD COLUMN status TEXT NOT NULL DEFAULT 'draft';
