-- One inbox per handle. Admission rejects a taken handle before it stores a
-- claim; this index is the backstop when that check is wrong.
CREATE UNIQUE INDEX identifier_association_handle
    ON identifier_association (identifier) WHERE identifier_kind = 3;
