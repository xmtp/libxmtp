import assert from "node:assert/strict";
import { test } from "node:test";

import { contend } from "./probeContend.ts";

test("contend leaves the message reader in the foreground, also when the rival cannot open", async () => {
  for (const opens of [true, false]) {
    let foreground = true;
    let ended = false;
    const work = contend({
      setForeground: (active) => {
        foreground = active;
        return Promise.resolve();
      },
      openRival: () => ({
        // A second contend while the first holds the lease: ConsumerOwned.
        ready: () =>
          opens ? Promise.resolve() : Promise.reject(new Error("owned")),
        end: () => {
          ended = true;
          return Promise.resolve();
        },
      }),
      sleep: () => Promise.resolve(),
    });
    if (opens) {
      await work;
      // The rival lets the lease go after the hold.
      assert.ok(ended);
    } else {
      await assert.rejects(work);
    }
    assert.equal(foreground, true, `rival opens: ${String(opens)}`);
  }
});
