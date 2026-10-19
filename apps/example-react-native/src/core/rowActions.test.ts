import assert from "node:assert/strict";
import { test } from "node:test";

import {
  type RowAction,
  SwipeGroup,
  menuAlertButtons,
  rowAccessibilityActions,
} from "./rowActions.ts";

class FakeRow {
  closes = 0;
  close(): void {
    this.closes += 1;
  }
}

const action = (label: string, destructive?: boolean): RowAction => ({
  label,
  ...(destructive === undefined ? {} : { destructive }),
  onPress: () => {},
});

test("opening a row closes the row that was open", () => {
  const group = new SwipeGroup<FakeRow>();
  const first = new FakeRow();
  const second = new FakeRow();
  group.opened(first);
  group.opened(first);
  assert.equal(first.closes, 0);
  group.opened(second);
  assert.equal(first.closes, 1);
  assert.equal(second.closes, 0);
});

test("a tap while a row is open only closes it; the next tap acts", () => {
  const group = new SwipeGroup<FakeRow>();
  const row = new FakeRow();
  assert.equal(group.closeAll(), false);
  group.opened(row);
  assert.equal(group.closeAll(), true);
  assert.equal(row.closes, 1);
  assert.equal(group.closeAll(), false);
  assert.equal(row.closes, 1);
});

test("a row that closed by itself no longer swallows taps", () => {
  const group = new SwipeGroup<FakeRow>();
  const first = new FakeRow();
  const second = new FakeRow();
  group.opened(first);
  group.opened(second);
  // The first row's close callback arrives after the second one opened.
  group.closed(first);
  assert.equal(group.closeAll(), true);
  assert.equal(second.closes, 1);
  group.opened(first);
  group.closed(first);
  assert.equal(group.closeAll(), false);
});

test("the Android menu adds Cancel only while it fits in three buttons", () => {
  const shown = (actions: readonly RowAction[]) =>
    menuAlertButtons(actions, "Cancel").map(({ text, style }) => ({
      text,
      style,
    }));
  assert.deepEqual(shown([action("Block", true)]), [
    { text: "Block", style: "destructive" },
    { text: "Cancel", style: "cancel" },
  ]);
  assert.deepEqual(
    shown([action("Accept"), action("Delete", true), action("Block", true)]),
    [
      { text: "Accept", style: undefined },
      { text: "Delete", style: "destructive" },
      { text: "Block", style: "destructive" },
    ],
  );
  assert.throws(
    () =>
      menuAlertButtons(
        [action("A"), action("B"), action("C"), action("D")],
        "Cancel",
      ),
    RangeError,
  );
});

test("VoiceOver gets every menu and swipe action once, in menu order", () => {
  const accept = action("Accept");
  const remove = action("Delete", true);
  const block = action("Block", true);
  assert.deepEqual(
    rowAccessibilityActions([accept, remove, block], [remove, block]).map(
      (entry) => entry.label,
    ),
    ["Accept", "Delete", "Block"],
  );
  assert.deepEqual(
    rowAccessibilityActions([], [block]).map((entry) => entry.label),
    ["Block"],
  );
});
