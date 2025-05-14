CREATE TABLE icebox (
  sequence_id BIGINT NOT NULL,
  originator_id BIGINT NOT NULL,
  depending_sequence_id BIGINT,
  depending_originator_id BIGINT,
  envelope_payload BLOB NOT NULL,
  PRIMARY KEY (sequence_id, originator_id),
  CHECK ((depending_sequence_id IS NULL AND depending_originator_id IS NULL) OR 
         (depending_sequence_id IS NOT NULL AND depending_originator_id IS NOT NULL))
);

CREATE INDEX idx_icebox_dependencies ON icebox(depending_sequence_id, depending_originator_id);
