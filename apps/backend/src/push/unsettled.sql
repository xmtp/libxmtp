SELECT COALESCE(
    (SELECT max(sequence_id) FROM envelopes WHERE push_eligible) > $1,
    false
) AS "above!"
