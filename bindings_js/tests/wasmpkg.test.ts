import { beforeEach, expect, it } from "vitest";
import { Client } from "..";

it("can instantiate multiple instances", async () => {
  const a = await Client.create();
  expect(a).toBeDefined();

  // Make sure we can call it again
  const b = await Client.create();
  expect(b).toBeDefined();

  expect(a).not.toEqual(b);
});

let client: Client;

beforeEach(async () => {
  Client.resetAll();
  client = await Client.create();
});

it("can read and write to in-memory storage", async () => {
  client.writeToPersistence("foo", new Uint8Array([1, 2, 3]));
  expect(client.readFromPersistence("foo")).toEqual(new Uint8Array([1, 2, 3]));
})
