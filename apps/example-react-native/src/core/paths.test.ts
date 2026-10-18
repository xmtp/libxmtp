import assert from "node:assert/strict";
import { test } from "node:test";

import {
  databaseFiles,
  isInside,
  joinPath,
  pathToUri,
  profilePaths,
  uriToPath,
} from "./paths.ts";

test("file:// URIs become native paths and back", () => {
  assert.equal(
    uriToPath("file:///var/mobile/a%20b/c.png"),
    "/var/mobile/a b/c.png",
  );
  assert.equal(uriToPath("file://localhost/tmp/x"), "/tmp/x");
  assert.equal(uriToPath("/already/a/path"), "/already/a/path");
  assert.equal(
    pathToUri("/var/mobile/a b/c#1.png"),
    "file:///var/mobile/a%20b/c%231.png",
  );
  assert.equal(pathToUri("file:///x"), "file:///x");
  const path = "/Users/me/Library/Application Support/messenger/ü #1";
  assert.equal(uriToPath(pathToUri(path)), path);
});

test("profile paths mirror the iOS layout", () => {
  const paths = profilePaths("/Support/messenger/", "6F1C");
  assert.deepEqual(paths, {
    root: "/Support/messenger/profiles/6F1C",
    database: "/Support/messenger/profiles/6F1C/sdk/messages.db3",
    attachments: "/Support/messenger/profiles/6F1C/sdk/attachments",
    temp: "/Support/messenger/profiles/6F1C/temp",
    preferences: "/Support/messenger/profiles/6F1C/prefs",
    exports: "/Support/messenger/profiles/6F1C/exports",
  });
  assert.deepEqual(databaseFiles(paths.database), [
    paths.database,
    `${paths.database}-wal`,
    `${paths.database}-shm`,
  ]);
});

test("joins segments and checks containment", () => {
  assert.equal(joinPath("/a/", "/b/", "c"), "/a/b/c");
  assert.equal(joinPath("/a", "", "c"), "/a/c");
  assert.ok(isInside("/root/profiles/x", "/root"));
  assert.ok(isInside("/root/a", "/root/"));
  assert.ok(!isInside("/root", "/root"));
  assert.ok(!isInside("/rooted/a", "/root"));
  assert.ok(!isInside("/root/../etc", "/root"));
});
