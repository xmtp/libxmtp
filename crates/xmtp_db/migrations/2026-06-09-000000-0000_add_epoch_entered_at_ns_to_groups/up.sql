-- XIP-82: the delivery-service envelope timestamp of the message by
-- which this client entered the current MLS epoch (the epoch's commit,
-- or the welcome for members added at that epoch). NULL means the group
-- has not advanced epoch since this column landed; readers fall back to
-- created_at_ns (the initial epoch). Consumed by the external-commit
-- validator's expire_in_ns staleness bound.
ALTER TABLE groups
ADD COLUMN epoch_entered_at_ns BIGINT;
