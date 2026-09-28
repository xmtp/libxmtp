-- The app list now selects live previews with a bound nanosecond time.
-- Retire the old view so it cannot return expired decrypted bytes.
DROP VIEW conversation_list;
