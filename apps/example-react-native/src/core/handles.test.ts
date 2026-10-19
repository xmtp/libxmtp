import assert from "node:assert/strict";
import { test } from "node:test";

import {
  type HandleEditorState,
  type HandleNotice,
  type HandleProblem,
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

test("problem and notice copy carries the handle, the minimum and the dates", () => {
  // Only interpolated values and branch differences are pinned here; the
  // wording itself may change.
  assert.match(handleProblemCopy("tooShort", "", 5), /\b5\b/u);
  assert.match(handleProblemCopy("badCode", "nick#12", 3), /nick#0012/u);
  assert.match(handleHint(4), /\b4\b/u);
  const problems: readonly HandleProblem[] = [
    "invalidCharacters",
    "startWithLetter",
    "tooLong",
    "tooShort",
    "badCode",
    "current",
  ];
  const problemCopy = new Set(problems.map((p) => handleProblemCopy(p, "", 3)));
  assert.equal(problemCopy.size, problems.length, "one text per problem");

  const named: readonly HandleNotice[] = [
    { kind: "taken", handle: "zed" },
    { kind: "notAllowed", name: "zed" },
    { kind: "claimFailed", handle: "zed" },
    { kind: "recordFailed", handle: "zed" },
    { kind: "switchBackBlocked", handle: "zed", shorter: true },
    { kind: "switchBackBlocked", handle: "zed", shorter: false },
  ];
  for (const notice of named) {
    assert.match(handleNoticeCopy(notice), /\bzed\b/u, notice.kind);
  }
  assert.notEqual(
    handleNoticeCopy({
      kind: "switchBackBlocked",
      handle: "zed",
      shorter: true,
    }),
    handleNoticeCopy({
      kind: "switchBackBlocked",
      handle: "zed",
      shorter: false,
    }),
  );
  const dated = handleNoticeCopy({
    kind: "tooSoon",
    changed: new Date(2026, 9, 18, 15, 0),
    next: new Date(2026, 9, 25, 15, 0),
  });
  assert.match(dated, /Oct 18, 2026.*Oct 25, 2026/u);
  const undated = handleNoticeCopy({
    kind: "tooSoon",
    changed: undefined,
    next: undefined,
  });
  assert.doesNotMatch(undated, /2026/u);
  const plain: readonly HandleNotice[] = [
    { kind: "limitReached" },
    { kind: "checkFailed" },
    { kind: "skipFailed" },
  ];
  const plainCopy = new Set([undated, ...plain.map(handleNoticeCopy)]);
  assert.equal(plainCopy.size, plain.length + 1, "one text per notice");

  const available: HandleStatus = { kind: "available", handle: "zed" };
  const claim = primaryButtonTitle("choose", editor({ status: available }));
  const change = primaryButtonTitle("change", editor({ status: available }));
  assert.match(claim, /\bzed\b/u);
  assert.match(change, /\bzed\b/u);
  assert.notEqual(claim, change, "the title depends on the mode");
  assert.match(
    primaryButtonTitle("change", editor({ status: available, claiming: "qq" })),
    /\bqq\b/u,
    "a running claim names its handle",
  );
  assert.doesNotMatch(primaryButtonTitle("choose", editor()), /\babc\b/u);
});
