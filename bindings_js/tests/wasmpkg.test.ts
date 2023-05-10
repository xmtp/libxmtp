import { beforeEach, expect, it } from "vitest";
import { Client } from "..";

it("can instantiate multiple instances", async () => {
  const a = await Client.createTest();
  expect(a).toBeDefined();

  // Make sure we can call it again
  const b = await Client.createTest();
  expect(b).toBeDefined();

  expect(a).not.toEqual(b);
});

let client: Client;

beforeEach(async () => {
  Client.resetAll();
  client = await Client.createTest();
});

it("can read and write to in-memory storage", async () => {
  client.writeToPersistence("foo", new Uint8Array([1, 2, 3]));
  expect(client.readFromPersistence("foo")).toEqual(new Uint8Array([1, 2, 3]));
})

it("throws appropriate errors", async () => {
  let getItem = window.localStorage.getItem;
  window.localStorage.getItem = () => { throw new Error("error") };
  client.writeToPersistence("foo", new Uint8Array([1, 2, 3]));
  expect(() => client.readFromPersistence("foo")).toThrow();
  window.localStorage.getItem = getItem;
})
