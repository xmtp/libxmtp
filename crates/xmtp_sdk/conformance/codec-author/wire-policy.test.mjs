import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";

// The observer is outside the codec package. It reads this worktree's backend
// and the public wire schemas, so SDK send-policy code cannot supply the result.
export function verifyWirePolicy({ conversationId, rawEnvelopes }) {
  assert.match(conversationId, /^[0-9a-f]{32}$/);
  const database = process.env.DATABASE_URL;
  assert.ok(
    database,
    "DATABASE_URL is required for the isolated wire observer",
  );
  const protoc = process.env.SDK_AUTHOR_PROTOC;
  assert.ok(protoc, "the recipe must supply protoc from the Rust Nix shell");
  const sql = `SELECT encode(payload, 'hex') FROM envelopes WHERE topic = decode('00${conversationId}', 'hex') AND NOT is_commit_or_proposal ORDER BY sequence_id`;
  const rows = execFileSync(
    "dev/docker/compose",
    [
      "exec",
      "-T",
      "db",
      "psql",
      "-U",
      "xmtp",
      "-d",
      "xmtp_backend",
      "-XAt",
      "-c",
      sql,
    ],
    {
      encoding: "utf8",
    },
  )
    .trim()
    .split("\n");
  const push = rows.map((hex) => {
    const envelope = execFileSync(
      protoc,
      [
        "-I",
        "proto",
        "--decode=xmtp.backend.v1.ClientEnvelope",
        "backend/v1/backend.proto",
      ],
      { input: Buffer.from(hex, "hex"), encoding: "utf8" },
    );
    assert.match(envelope, /^group_message \{/);
    return /^\s*should_push: true$/m.test(envelope);
  });
  // Custom hook false, explicit send false, reply catalogue true, explicit reply false.
  // Exactly four publications also proves failed encode/fallback/push steps publish nothing.
  assert.deepEqual(
    push,
    [false, false, true, false],
    "published push policy or publication count changed",
  );
  assert.equal(rawEnvelopes.length, 6);
  for (const bytes of rawEnvelopes) {
    const encoded = execFileSync(
      protoc,
      [
        "-I",
        "proto",
        "--decode=xmtp.message_contents.EncodedContent",
        "message_contents/content.proto",
      ],
      { input: Buffer.from(bytes), encoding: "utf8" },
    );
    assert.doesNotMatch(
      encoded,
      /^\s*compression:/m,
      "send compressed content without opt-in",
    );
  }
}
