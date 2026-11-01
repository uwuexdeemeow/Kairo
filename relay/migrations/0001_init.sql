-- A group is the computers linked by one key. The relay keeps only the access token's hash.
CREATE TABLE groups (
  id TEXT PRIMARY KEY,
  token_hash TEXT NOT NULL,
  created_ms INTEGER NOT NULL,
  used_ms INTEGER NOT NULL,
  bytes INTEGER NOT NULL DEFAULT 0
);

-- Each computer's sealed files, under sealed names.
CREATE TABLE files (
  group_id TEXT NOT NULL,
  computer TEXT NOT NULL,
  name TEXT NOT NULL,
  version INTEGER NOT NULL,
  data BLOB NOT NULL,
  size INTEGER NOT NULL,
  PRIMARY KEY (group_id, computer, name)
);

-- A pairing in progress: SPAKE2 messages and the sealed invitation and reply, each written once.
CREATE TABLE pairs (
  slot TEXT PRIMARY KEY,
  a TEXT NOT NULL,
  b TEXT,
  invite TEXT,
  reply TEXT,
  expires_ms INTEGER NOT NULL
);
