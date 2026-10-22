import assert from "node:assert/strict";
import { test } from "node:test";

import { LOAD_FAILED } from "./conversationList.ts";
import { RecoveryChatsStore } from "./recoveryChats.ts";

test("a failed chat list shows the load error until Retry loads the choices", async (t) => {
  t.mock.method(console, "warn", () => {});
  let offline = true;
  const store = new RecoveryChatsStore({
    queries: {
      list: (consent) =>
        offline
          ? Promise.reject(new Error("offline"))
          : Promise.resolve([
              { id: "g1", kind: "group", consent, preview: "", unread: 0 },
            ]),
    },
    conversation: (id) => Promise.resolve({ id, kind: "group", name: "Plans" }),
    alias: () => undefined,
    admit: () => true,
  });
  await store.load();
  assert.deepEqual(store.getState().chats, []);
  assert.equal(store.getState().error, LOAD_FAILED);

  // Retry hides the error while it runs, then shows the choices.
  offline = false;
  const retry = store.load();
  assert.equal(store.getState().error, undefined);
  await retry;
  assert.equal(store.getState().error, undefined);
  assert.deepEqual(store.getState().chats, [{ id: "g1", title: "Plans" }]);
});
