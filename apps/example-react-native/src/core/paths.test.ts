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

test("the saved profile layout stays readable by later builds", () => {
  const paths = profilePaths("/S/messenger", "6F1C");
  // Installed apps hold files at these paths; a change needs a migration.
  assert.deepEqual(paths, {
    root: "/S/messenger/profiles/6F1C",
    database: "/S/messenger/profiles/6F1C/sdk/messages.db3",
    attachments: "/S/messenger/profiles/6F1C/sdk/attachments",
    temp: "/S/messenger/profiles/6F1C/temp",
    preferences: "/S/messenger/profiles/6F1C/prefs",
    exports: "/S/messenger/profiles/6F1C/exports",
  });
  // Delete local data removes the root, so every path must be inside it.
  const others = Object.entries(paths).filter(([name]) => name !== "root");
  for (const [name, path] of others)
    assert.ok(isInside(path, paths.root), name);
  assert.equal(new Set(others.map(([, path]) => path)).size, others.length);
  assert.deepEqual(databaseFiles(paths.database), [
    paths.database,
    `${paths.database}-wal`,
    `${paths.database}-shm`,
    `${paths.database}.sqlcipher_salt`,
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
