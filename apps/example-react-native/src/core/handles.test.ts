import assert from "node:assert/strict";
import { test } from "node:test";

import {
  type HandleEditorState,
  type HandleNotice,
  type HandleStatus,
  availabilityStatus,
  canCheck,
  canClaim,
  handleHint,
  handleName,
  handleNoticeCopy,
  handleProblem,
  handleProblemCopy,
  hasMoreSuggestions,
  isEditable,
  normalizeHandle,
  primaryButtonTitle,
  selectSuggestion,
  showMoreSuggestions,
  statusPick,
  tooSoonNotice,
  visibleSuggestions,
} from "./handles.ts";

test("the first grammar problem shown follows the check order", () => {
  const cases: readonly (readonly [string, number, string | undefined])[] = [
    // One problem per row, in the order the checks run.
    ["a!#x", 3, "invalidCharacters"],
    ["1#x", 3, "startWithLetter"],
    ["a".repeat(28) + "#x", 3, "tooLong"],
    ["a#x", 3, "tooShort"],
    ["abc#x", 3, "badCode"],
    ["abc#1234", 3, undefined],
    // Edges.
    ["", 3, undefined],
    ["Abc", 3, "invalidCharacters"],
    ["_abc", 3, "startWithLetter"],
    ["a".repeat(27), 3, undefined],
    // Length checks run longest first, even when the minimum is higher.
    ["a".repeat(28), 30, "tooLong"],
    ["ab", 3, "tooShort"],
    ["a", 1, undefined],
    ["abc#", 3, "badCode"],
    ["abc#12345", 3, "badCode"],
    ["abc#12a4", 3, "badCode"],
    ["abc#١٢٣٤", 3, "badCode"],
    ["a-b_c9", 3, undefined],
  ];
  for (const [input, minimum, problem] of cases) {
    assert.equal(handleProblem(input, minimum), problem, input);
  }
  assert.equal(normalizeHandle("AbÇ"), "abÇ");
  assert.equal(handleName("nick#2341"), "nick");
  assert.equal(handleName("nick"), "nick");
});

test("problem and notice copy matches HandleCopy.swift", () => {
  assert.equal(
    handleProblemCopy("invalidCharacters", "", 3),
    "Use only lowercase letters, numbers, _ and -.",
  );
  assert.equal(
    handleProblemCopy("startWithLetter", "", 3),
    "Start with a letter.",
  );
  assert.equal(
    handleProblemCopy("tooLong", "", 3),
    "Use 27 characters or fewer before #.",
  );
  assert.equal(
    handleProblemCopy("tooShort", "", 5),
    "Use at least 5 characters on this server.",
  );
  assert.equal(
    handleProblemCopy("badCode", "nick#12", 3),
    "After #, use exactly 4 digits, like nick#0012.",
  );
  assert.equal(
    handleProblemCopy("current", "", 3),
    "This is your current handle.",
  );
  assert.equal(handleHint(3), "3 to 27 lowercase letters, numbers, _ or -.");
  const notices: readonly (readonly [HandleNotice, string])[] = [
    [
      { kind: "taken", handle: "abc" },
      "Someone claimed abc a moment before you. Pick another handle.",
    ],
    [{ kind: "notAllowed", name: "abc" }, "abc is not allowed on this server."],
    [
      { kind: "tooSoon", changed: undefined, next: undefined },
      "You changed your handle less than 7 days ago. Try again later.",
    ],
    [
      {
        kind: "tooSoon",
        changed: new Date(2026, 9, 18, 15, 0),
        next: new Date(2026, 9, 25, 15, 0),
      },
      "You changed your handle on Oct 18, 2026. You can change it again on Oct 25, 2026.",
    ],
    [
      { kind: "limitReached" },
      "This inbox has used all 64 handle changes. Your current handle stays, and so do your other handles.",
    ],
    [{ kind: "checkFailed" }, "Cannot check this handle. Try again."],
    [{ kind: "skipFailed" }, "Cannot save your preference. Try again."],
    [{ kind: "claimFailed", handle: "abc" }, "Cannot claim abc. Try again."],
    [
      { kind: "recordFailed", handle: "abc" },
      "Your handle is abc, but its change date could not be saved. Retry to save it on this device.",
    ],
    [
      { kind: "switchBackBlocked", handle: "ab", shorter: true },
      "ab is shorter than this server now allows. It still finds you, but you cannot switch back to it.",
    ],
    [
      { kind: "switchBackBlocked", handle: "ab", shorter: false },
      "ab is not allowed on this server now. It still finds you, but you cannot switch back to it.",
    ],
  ];
  for (const [notice, copy] of notices)
    assert.equal(handleNoticeCopy(notice), copy);
});

test("tooSoon uses the newest claim under 7 days old", () => {
  const now = new Date(2026, 9, 18, 12, 0);
  const day = 86_400_000;
  const recent = new Date(now.getTime() - 2 * day);
  const old = new Date(now.getTime() - 9 * day);
  assert.deepEqual(tooSoonNotice([old, recent], now), {
    kind: "tooSoon",
    changed: recent,
    next: new Date(recent.getTime() + 7 * day),
  });
  assert.deepEqual(tooSoonNotice([old], now), {
    kind: "tooSoon",
    changed: undefined,
    next: undefined,
  });
  assert.deepEqual(tooSoonNotice([], now), {
    kind: "tooSoon",
    changed: undefined,
    next: undefined,
  });
});

test("availability: the input itself, else up to 3 suggestions without the taken handle, else none free", () => {
  assert.deepEqual(
    availabilityStatus("abc", ["abc", "abc#0001"], ["abc", "abc#0001"]),
    { kind: "available", handle: "abc" },
  );
  const answers = ["abc#0001", "abc#0002", "abc#0003", "abc#0004"];
  const status = availabilityStatus("abc", answers, answers);
  assert.deepEqual(visibleSuggestions(status), [
    "abc#0001",
    "abc#0002",
    "abc#0003",
  ]);
  assert.equal(statusPick(status), "abc#0001");
  // Select picks only a visible suggestion.
  assert.equal(statusPick(selectSuggestion(status, "abc#0002")), "abc#0002");
  assert.equal(statusPick(selectSuggestion(status, "abc#0004")), "abc#0001");
  assert.ok(hasMoreSuggestions(status));
  const more = showMoreSuggestions(status);
  assert.equal(visibleSuggestions(more).length, 4);
  assert.ok(!hasMoreSuggestions(more));
  assert.deepEqual(availabilityStatus("abc#1234", [], []), {
    kind: "noneFree",
    input: "abc#1234",
  });
  // A handle that was just taken is never offered, even as the free input.
  const retaken = availabilityStatus("abc", answers, answers, "abc#0001");
  assert.ok(!visibleSuggestions(retaken).includes("abc#0001"));
  assert.equal(statusPick(retaken), "abc#0002");
  assert.equal(
    availabilityStatus("abc", ["abc"], ["abc"], "abc").kind,
    "noneFree",
  );
});

const editor = (
  overrides: Partial<HandleEditorState> = {},
): HandleEditorState => ({
  loading: false,
  input: "abc",
  problem: undefined,
  status: { kind: "editing" },
  claiming: undefined,
  notice: undefined,
  ...overrides,
});

test("editor flags decide when a handle can be checked, edited and claimed", () => {
  assert.ok(canCheck(editor()));
  assert.ok(!canCheck(editor({ loading: true })));
  assert.ok(!canCheck(editor({ input: "" })));
  assert.ok(!canCheck(editor({ problem: "tooShort" })));
  assert.ok(!canCheck(editor({ notice: { kind: "notAllowed", name: "abc" } })));
  assert.ok(canCheck(editor({ notice: { kind: "notAllowed", name: "xyz" } })));
  assert.ok(!isEditable(editor({ notice: { kind: "limitReached" } })));
  assert.ok(
    !isEditable(editor({ notice: { kind: "recordFailed", handle: "abc" } })),
  );
  assert.ok(!isEditable(editor({ claiming: "abc" })));
  const available: HandleStatus = { kind: "available", handle: "abc" };
  assert.ok(canClaim(editor({ status: available })));
  assert.ok(!canClaim(editor({ status: available, claiming: "abc" })));
});

test("primary button titles", () => {
  const available: HandleStatus = { kind: "available", handle: "abc" };
  assert.equal(primaryButtonTitle("choose", editor()), "Continue");
  assert.equal(
    primaryButtonTitle("choose", editor({ status: available })),
    "Claim abc",
  );
  assert.equal(
    primaryButtonTitle("change", editor({ status: available })),
    "Change to abc",
  );
  assert.equal(
    primaryButtonTitle("change", editor({ claiming: "abc" })),
    "Claiming abc…",
  );
});
