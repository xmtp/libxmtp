import assert from "node:assert/strict";
import { test } from "node:test";

import { Store, ValueStore } from "./store.ts";

class Counter extends Store<{ readonly count: number }> {
  constructor() {
    super({ count: 0 });
  }

  increment(): void {
    this.setState((state) => ({ count: state.count + 1 }));
  }

  same(): void {
    this.setState((state) => state);
  }
}

test("notifies subscribers of each change until they unsubscribe", () => {
  const counter = new Counter();
  let calls = 0;
  const unsubscribe = counter.subscribe(() => {
    calls += 1;
  });
  counter.increment();
  const before = counter.getState();
  counter.same();
  assert.equal(counter.getState(), before);
  assert.equal(calls, 1, "the same state object notifies nobody");
  counter.increment();
  assert.equal(counter.getState().count, 2);
  assert.equal(calls, 2);
  unsubscribe();
  counter.increment();
  assert.equal(calls, 2);
  assert.equal(counter.getState().count, 3);
});

test("a listener that unsubscribes during notification does not skip others", () => {
  const store = new ValueStore(0);
  const seen: string[] = [];
  const first = store.subscribe(() => {
    seen.push("first");
    first();
  });
  store.subscribe(() => {
    seen.push("second");
  });
  store.set(1);
  store.set(2);
  assert.deepEqual(seen, ["first", "second", "second"]);
});
