-- Listing only what changed. Each group counts its writes, and a file's version is the count when
-- it was last written, so "since" one number finds every change after it through an index. A
-- file deleted leaves a note for a month, so computers listing what changed hear that it went;
-- `purged_seq` is the newest note forgotten, before which only a full list will do.
ALTER TABLE groups ADD COLUMN seq INTEGER NOT NULL DEFAULT 0;
ALTER TABLE groups ADD COLUMN purged_seq INTEGER NOT NULL DEFAULT 0;
UPDATE groups SET seq = COALESCE((SELECT MAX(version) FROM files WHERE files.group_id = groups.id), 0);

CREATE TABLE gone_files (
  group_id TEXT NOT NULL,
  computer TEXT NOT NULL,
  name TEXT NOT NULL,
  version INTEGER NOT NULL,
  gone_ms INTEGER NOT NULL,
  PRIMARY KEY (group_id, computer, name)
);

CREATE INDEX files_by_version ON files (group_id, version);
CREATE INDEX gone_by_version ON gone_files (group_id, version);
CREATE INDEX gone_by_time ON gone_files (gone_ms);
