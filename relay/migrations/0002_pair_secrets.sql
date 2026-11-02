-- Who may write a pairing slot's later parts: the hash of the host's secret, sent when it opens the
-- slot, and of the answering computer's, sent with its answer. Slots opened without one (by Kairo
-- before 1.26.5) work as before.
ALTER TABLE pairs ADD COLUMN host_hash TEXT;
ALTER TABLE pairs ADD COLUMN joiner_hash TEXT;
