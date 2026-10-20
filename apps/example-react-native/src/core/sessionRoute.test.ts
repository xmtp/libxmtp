import assert from "node:assert/strict";
import { test } from "node:test";

import { closedSessionRoute, openSessionRoute } from "./sessionRoute.ts";

test("starting and opening keep the current screen", () => {
  for (const path of ["/", "/connect", "/chat/abc"]) {
    assert.deepEqual(closedSessionRoute("starting", path), { kind: "stay" });
    assert.deepEqual(closedSessionRoute("opening", path), { kind: "stay" });
  }
});

test("every state without a session shows Connect", () => {
  for (const state of [
    "signedOut",
    "restoreFailed",
    "resetIncomplete",
  ] as const) {
    assert.deepEqual(closedSessionRoute(state, "/"), {
      kind: "replace",
      path: "/connect",
    });
    assert.deepEqual(closedSessionRoute(state, "/settings"), {
      kind: "replace",
      path: "/connect",
    });
    assert.deepEqual(closedSessionRoute(state, "/connect"), { kind: "stay" });
  }
});

test("a storage failure returns to the startup screen", () => {
  assert.deepEqual(closedSessionRoute("storageFailed", "/chat/abc"), {
    kind: "replace",
    path: "/",
  });
  assert.deepEqual(closedSessionRoute("storageFailed", "/"), { kind: "stay" });
});

test("an open session replaces the startup and Connect roots", () => {
  assert.deepEqual(openSessionRoute("chats", "/"), {
    kind: "replace",
    path: "/chats",
  });
  assert.deepEqual(openSessionRoute("chats", "/connect"), {
    kind: "replace",
    path: "/chats",
  });
  assert.deepEqual(openSessionRoute("choose-handle", "/connect"), {
    kind: "replace",
    path: "/choose-handle",
  });
  assert.deepEqual(openSessionRoute("chats", "/chats"), { kind: "stay" });
});

test("a launch deep link is pushed again on top of Chats", () => {
  assert.deepEqual(openSessionRoute("chats", "/chat/abc"), {
    kind: "replace",
    path: "/chats",
    push: "/chat/abc",
  });
  assert.deepEqual(openSessionRoute("choose-handle", "/chat/abc"), {
    kind: "replace",
    path: "/choose-handle",
  });
});

test("Debug tools are never redirected", () => {
  assert.deepEqual(closedSessionRoute("signedOut", "/dev/gallery"), {
    kind: "stay",
  });
  assert.deepEqual(openSessionRoute("chats", "/dev/session-probe"), {
    kind: "stay",
  });
  assert.deepEqual(closedSessionRoute("signedOut", "/devices"), {
    kind: "replace",
    path: "/connect",
  });
});
