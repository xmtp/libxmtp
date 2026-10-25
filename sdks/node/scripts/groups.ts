import { readFile } from "node:fs/promises";
import path from "node:path";

import { createSigner, createUser, type User } from "@test/helpers";
import {
  Client,
  type ClientOptions,
  type ListConversationsOptions,
  type Timestamp,
} from "@xmtp/node-sdk";

/** The largest page the SDK returns. */
const CONVERSATION_PAGE_LIMIT = 1000;

/** Read every page of a creation-order list, newest first. */
const listAll = async <T extends { id: string; createdAt: Timestamp }>(
  list: (options: ListConversationsOptions) => Promise<T[]>,
) => {
  const all: T[] = [];
  for (;;) {
    const last = all.at(-1);
    const page = await list({
      orderBy: "createdAt",
      createdBefore: last?.createdAt,
      beforeId: last?.id,
      limit: CONVERSATION_PAGE_LIMIT,
    });
    all.push(...page);
    if (page.length < CONVERSATION_PAGE_LIMIT) return all;
  }
};

export const createRegisteredClient = async (
  user: User,
  dbPath?: string | null,
) => {
  const backendUrl = process.env.XMTP_BACKEND_URL;
  if (!backendUrl) throw new Error("XMTP_BACKEND_URL is required");
  const options: ClientOptions = {
    backend: { url: backendUrl },
    deviceSync: false,
    storage: {
      location: dbPath
        ? { dbPath, attachmentsDir: `${dbPath}.attachments` }
        : "inMemory",
    },
  };
  return Client.create(createSigner(user).signer, options);
};

const accountsJsonPath = path.join(import.meta.dirname, "accounts.json");
const parsedAccounts = JSON.parse(
  await readFile(accountsJsonPath, "utf-8"),
) as Record<string, string>;

type Account = { key: string; address: string };
const accounts: Account[] = Object.entries(parsedAccounts).map(
  ([key, address]) => ({ key, address }),
);

const primaryAccount = accounts.shift() as Account;

const primaryAccountClient = await createRegisteredClient(
  createUser(primaryAccount.key as `0x${string}`),
  "./test.db3",
);

console.log("Registering accounts...");

for (const a of accounts) {
  await createRegisteredClient(createUser(a.key as `0x${string}`), null);
}

const groups = [];

console.log("Creating groups...");

// create a bunch of groups
while (accounts.length > 200) {
  const groupsAccounts = accounts.splice(0, 4);
  const group = await primaryAccountClient.conversations.createGroup(
    groupsAccounts.map((a) => ({
      kind: "ethereum" as const,
      identifier: a.address,
    })),
  );
  groups.push(group);
}

console.log(`Created ${groups.length} groups`);

console.log(`Sending "gm" message into each group...`);

for (const group of groups) {
  await group.sendText("gm");
}

console.log("Creating DM groups...");

const dmGroups = [];

while (accounts.length > 0) {
  const dmGroup = await primaryAccountClient.conversations.createDm({
    kind: "ethereum" as const,
    identifier: (accounts.pop() as Account).address,
  });
  dmGroups.push(dmGroup);
}

console.log(`Created ${dmGroups.length} DM groups`);

console.log("Sending 'gm' message into each DM group...");

for (const dmGroup of dmGroups) {
  await dmGroup.sendText("gm");
}

console.log("Syncing all conversations...");

await primaryAccountClient.conversations.syncAll(undefined);

console.log("Querying DM groups...");

const groupConvos = await listAll((options) =>
  primaryAccountClient.conversations.listGroups(options),
);
const dmConvos = await listAll((options) =>
  primaryAccountClient.conversations.listDms(options),
);

console.log(`Found ${dmConvos.length} DM conversations`);
console.log(`Found ${groupConvos.length} group conversations`);
