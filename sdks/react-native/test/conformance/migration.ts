import {
  encodeEncodedContent,
  prepareMigrationArchive,
  Group,
  XmtpError,
  type Client,
} from "@xmtp/react-native-sdk";
import { NativeModules } from "react-native";

import { assert, bytesEqual, equal } from "./assertions";

interface SourceState {
  readonly [suffix: string]: string | null;
}
interface NativeFixture {
  prepare(): Promise<{
    databasePath: string;
    outputPath: string;
    expectedJson: string;
    inspectionJson: string;
    sourceState: SourceState;
    unsupportedDatabasePath: string;
    unsupportedJson: string;
    unsupportedState: SourceState;
  }>;
  sourceState(unsupported: boolean): Promise<SourceState>;
  exists(path: string): Promise<boolean>;
}
interface ExpectedRecords {
  counts: { groups: number; messages: number; consent: number };
  databaseKeyHex: string;
  groups: readonly {
    id: string;
    name: string;
    description: string;
    createdAtNs: string;
  }[];
  messages: readonly {
    id: string;
    groupId: string;
    senderInboxId: string;
    sentAtNs: string;
    encodedContentHex: string;
    text: string;
  }[];
  consent: readonly {
    entityType: "conversation_id" | "inbox_id";
    entity: string;
    state: "allowed" | "denied";
  }[];
}

const unhex = (value: string): Uint8Array => {
  assert(
    value.length % 2 === 0 && /^[0-9a-f]+$/i.test(value),
    "fixture hex must have whole bytes",
  );
  return new Uint8Array(
    value.match(/../g)!.map((byte) => Number.parseInt(byte, 16)),
  );
};

/** Converts a closed DB made by the exact native source embedded by prior RN. */
export async function migrationJourney(
  client: Client,
): Promise<readonly string[]> {
  const native = NativeModules.XmtpMigrationFixture as
    | NativeFixture
    | undefined;
  assert(
    native,
    "installed fixture must provide native source-file preparation",
  );
  const fixture = await native.prepare();
  const expected = JSON.parse(fixture.expectedJson) as ExpectedRecords;
  const inspection = JSON.parse(fixture.inspectionJson) as {
    sourceHashesBefore: Record<string, { sha256: string }>;
  };
  equal(
    fixture.sourceState[""],
    inspection.sourceHashesBefore["react-native-5.7.0.db3"]?.sha256,
    "copied prior database must match independent source hash",
  );
  equal(
    fixture.sourceState[".sqlcipher_salt"],
    inspection.sourceHashesBefore["react-native-5.7.0.db3.sqlcipher_salt"]
      ?.sha256,
    "copied salt must match independent source hash",
  );
  for (const suffix of ["-wal", "-shm", "-journal"])
    equal(
      fixture.sourceState[suffix],
      null,
      "closed producer must retain absent sidecars",
    );
  const keyBuffer = new Uint8Array(96).fill(0x55);
  const databaseKey = keyBuffer.subarray(7, 39);
  databaseKey.set(unhex(expected.databaseKeyHex));
  const archiveKey = keyBuffer.subarray(49, 81).fill(13);
  const input = {
    databasePath: fixture.databasePath,
    databaseKey,
    archiveKey,
    outputPath: fixture.outputPath,
  };
  const report = await prepareMigrationArchive(input);
  equal(
    report.archivePath,
    fixture.outputPath,
    "RN converter must forward the explicit private output path",
  );
  equal(
    report.groupCount,
    BigInt(expected.counts.groups),
    "RN converter must retain bigint group count",
  );
  equal(
    report.messageCount,
    BigInt(expected.counts.messages),
    "RN converter must retain bigint history count",
  );
  equal(
    report.consentCount,
    BigInt(expected.counts.consent),
    "RN converter must retain bigint consent count",
  );
  const currentInbox = client.inboxId;
  await client.archives.importFromFile(report.archivePath, archiveKey);
  equal(
    client.inboxId,
    currentInbox,
    "history import must preserve the current installation's network identity",
  );
  for (const record of expected.groups) {
    const conversation = await client.conversations.getById(record.id);
    assert(
      conversation instanceof Group,
      "archive import must restore prior group history",
    );
    const state = await conversation.state();
    equal(
      state.name,
      record.name,
      "archive import must retain independently recorded group name",
    );
    equal(
      state.description,
      record.description,
      "archive import must retain group description",
    );
  }
  for (const record of expected.messages) {
    const message = await client.conversations.getMessageById(record.id);
    assert(message, "archive import must retain each independent message id");
    equal(
      message.senderInboxId,
      record.senderInboxId,
      "archive import must retain prior sender",
    );
    equal(
      message.sentAt.ns,
      BigInt(record.sentAtNs),
      "archive import must retain full-width timestamp",
    );
    assert(
      message.content.kind === "text",
      "archive import must decode prior text content",
    );
    equal(
      message.content.value,
      record.text,
      "archive import must retain independent prior text",
    );
    assert(
      message.encoded,
      "archive import must retain prior content envelope",
    );
    bytesEqual(
      encodeEncodedContent(message.encoded),
      Array.from(unhex(record.encodedContentHex)),
      "archive import must retain independent prior content bytes",
    );
  }
  for (const record of expected.consent) {
    const entity =
      record.entityType === "conversation_id"
        ? { kind: "conversation" as const, conversationId: record.entity }
        : { kind: "inbox" as const, inboxId: record.entity };
    equal(
      await client.preferences.consentState(entity),
      record.state,
      "archive import must retain independent prior consent",
    );
  }
  let rejection: unknown;
  try {
    await prepareMigrationArchive({
      ...input,
      databaseKey: new Uint8Array(32),
      outputPath: `${fixture.outputPath}.wrong-key`,
    });
  } catch (error) {
    rejection = error;
  }
  assert(
    rejection instanceof XmtpError.InvalidInput,
    "wrong legacy database key must preserve the typed converter error",
  );
  equal(
    await native.exists(`${fixture.outputPath}.wrong-key`),
    false,
    "wrong key must publish no archive",
  );
  const unsupported = JSON.parse(fixture.unsupportedJson) as {
    sourceFiles: Record<string, { sha256: string }>;
  };
  equal(
    fixture.unsupportedState[""],
    unsupported.sourceFiles["unsupported-schema.db3"]?.sha256,
    "negative schema copy must match independently recorded mutation hash",
  );
  equal(
    fixture.unsupportedState[".sqlcipher_salt"],
    unsupported.sourceFiles["unsupported-schema.db3.sqlcipher_salt"]?.sha256,
    "negative schema salt must match source ledger",
  );
  const unsupportedOutput = `${fixture.outputPath}.unsupported`;
  let unsupportedError: unknown;
  try {
    await prepareMigrationArchive({
      ...input,
      databasePath: fixture.unsupportedDatabasePath,
      outputPath: unsupportedOutput,
    });
  } catch (error) {
    unsupportedError = error;
  }
  assert(
    unsupportedError instanceof XmtpError.MigrationUnsupportedSchema,
    "unsupported migration inventory must preserve exact native typed error",
  );
  equal(
    unsupportedError.details.code,
    "MigrationUnsupportedSchema",
    "unsupported schema error must expose its public code",
  );
  equal(
    await native.exists(unsupportedOutput),
    false,
    "unsupported schema must publish no archive",
  );
  const unsupportedAfter = await native.sourceState(true);
  for (const suffix of ["", ".sqlcipher_salt", "-wal", "-shm", "-journal"]) {
    equal(
      unsupportedAfter[suffix],
      fixture.unsupportedState[suffix],
      "unsupported schema must preserve source hash and absent sidecars",
    );
    if (suffix.startsWith("-"))
      equal(
        unsupportedAfter[suffix],
        null,
        "unsupported schema source must remain cleanly closed",
      );
  }
  const after = await native.sourceState(false);
  for (const suffix of ["", ".sqlcipher_salt", "-wal", "-shm", "-journal"])
    equal(
      after[suffix],
      fixture.sourceState[suffix],
      "success and failure must preserve exact legacy source and sidecar absence",
    );
  return [
    "prior RN embedded-source database converts/imports exact history, bytes, timestamps and consent without changing network identity",
    "converter key views, bigint counts, distinct wrong-key/unsupported-schema typed failures, no partial output and source/sidecar preservation",
  ];
}
