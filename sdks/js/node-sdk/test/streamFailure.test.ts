import { describe, expect, it } from "vitest";
import { getStreamFailureDetails } from "../src/utils/streamFailure";

const marker = "\n[XMTP_STREAM_FAILURE_V1]";
const topic = () => ({
  topic: "00ff12",
  target: null as string | null,
  received: "9007199254740993",
  processed: "9007199254740992",
  unresolvedWelcomes: ["18446744073709551615"],
  inactive: false,
  cause: {
    kind: "target_pending",
    code: null as string | null,
    message: "Target capture is pending",
    retryable: true,
  },
});
const failure = () => ({
  kind: "barrier",
  code: "BarrierError::Deadline",
  message: "Stream processing did not reach the target",
  retryable: true,
  intentId: null as number | null,
  publishedIntentIds: [] as number[],
  summary: null as {
    messages: string;
    conversations: string;
    failed: string;
    completed: boolean;
  } | null,
  barriers: [{ reason: "deadline", unfinished: [topic()] }],
});
const error = (payload: unknown) =>
  new Error(
    `[BarrierError::Deadline] Stream processing failed${marker}${JSON.stringify(payload)}`,
  );

describe("structured stream failures", () => {
  it("retains an uncaptured target and exact large sequence values", () => {
    const details = getStreamFailureDetails(error(failure()));
    expect(details?.kind).toBe("barrier");
    expect(details?.barriers[0].unfinished[0]).toEqual({
      topic: new Uint8Array([0, 255, 18]),
      target: null,
      received: 9007199254740993n,
      processed: 9007199254740992n,
      unresolvedWelcomes: [18446744073709551615n],
      inactive: false,
      cause: {
        kind: "target_pending",
        code: null,
        message: "Target capture is pending",
        retryable: true,
      },
    });
  });

  it("distinguishes an empty captured target from failed target capture", () => {
    const payload = failure();
    payload.barriers[0].unfinished[0].target = "0";
    expect(
      getStreamFailureDetails(error(payload))?.barriers[0].unfinished[0].target,
    ).toBe(0n);
  });

  it("retains blocked and pending obligations in the same failure", () => {
    const payload = failure();
    const blocked = topic();
    blocked.target = "7";
    blocked.cause = {
      kind: "blocked",
      code: "GroupError::InvalidGroup",
      message: "Processing is blocked",
      retryable: false,
    };
    payload.barriers[0].unfinished.push(blocked);
    const obligations = getStreamFailureDetails(error(payload))?.barriers[0]
      .unfinished;
    expect(obligations).toHaveLength(2);
    expect(obligations?.[0].cause?.kind).toBe("target_pending");
    expect(obligations?.[1].cause).toEqual(blocked.cause);
  });

  it("retains the published intent when confirmation did not complete", () => {
    const payload = failure();
    payload.kind = "published_but_unconfirmed";
    payload.code = "GroupError::PublishedButUnconfirmed";
    payload.intentId = 42;
    payload.publishedIntentIds = [42];
    const details = getStreamFailureDetails(error(payload));
    expect(details?.kind).toBe("published_but_unconfirmed");
    expect(details?.intentId).toBe(42);
    expect(details?.publishedIntentIds).toEqual([42]);
    expect(details?.barriers).toHaveLength(1);
  });

  it("retains every sibling barrier and published intent in a sync summary", () => {
    const payload = failure();
    payload.kind = "catch_up";
    payload.summary = {
      messages: "9007199254740993",
      conversations: "4",
      failed: "2",
      completed: false,
    };
    payload.publishedIntentIds = [42, 43];
    payload.barriers.push({ reason: "cancelled", unfinished: [topic()] });
    const details = getStreamFailureDetails(error(payload));
    expect(details?.summary).toEqual({
      messages: 9007199254740993n,
      conversations: 4n,
      failed: 2n,
      completed: false,
    });
    expect(details?.publishedIntentIds).toEqual([42, 43]);
    expect(details?.barriers.map((barrier) => barrier.reason)).toEqual([
      "deadline",
      "cancelled",
    ]);
  });

  it("reads messages copied across a worker boundary", () => {
    const transferred = structuredClone(error(failure()));
    const message = transferred.message;
    expect(getStreamFailureDetails(transferred)?.kind).toBe("barrier");
    expect(getStreamFailureDetails({ message })?.kind).toBe("barrier");
    expect(getStreamFailureDetails(message)?.kind).toBe("barrier");
  });

  it("uses the last payload marker in a wrapped error", () => {
    const message = `Earlier text${marker}invalid\n${error(failure()).message}`;
    expect(getStreamFailureDetails(message)?.kind).toBe("barrier");
  });

  it.each([
    null,
    undefined,
    {},
    [],
    new Error("No stream details"),
    { message: 42 },
    `${marker}{`,
    `${marker}null`,
    `${marker}[]`,
  ])("returns undefined for missing or invalid details: %s", (value) => {
    expect(getStreamFailureDetails(value)).toBeUndefined();
  });

  it("does not throw if an error message getter fails", () => {
    const value = {
      get message() {
        throw new Error("Unavailable message");
      },
    };
    expect(getStreamFailureDetails(value)).toBeUndefined();
  });

  it.each([
    "-1",
    "01",
    "1.5",
    "1e3",
    "18446744073709551616",
    "",
    9007199254740992,
    null,
  ])("rejects an invalid sequence: %s", (value) => {
    const payload = failure();
    const unfinished = { ...topic(), received: value };
    expect(
      getStreamFailureDetails(
        error({
          ...payload,
          barriers: [{ reason: "deadline", unfinished: [unfinished] }],
        }),
      ),
    ).toBeUndefined();
  });

  it.each([
    { kind: "unknown" },
    { retryable: "false" },
    { intentId: 1.5 },
    { publishedIntentIds: [2147483648] },
    { summary: { messages: 1 } },
    { barriers: [{ reason: "unknown", unfinished: [] }] },
    {
      barriers: [
        { reason: "deadline", unfinished: [{ ...topic(), topic: "0" }] },
      ],
    },
    {
      barriers: [
        { reason: "deadline", unfinished: [{ ...topic(), target: undefined }] },
      ],
    },
    {
      barriers: [
        {
          reason: "deadline",
          unfinished: [
            { ...topic(), cause: { ...topic().cause, kind: "unknown" } },
          ],
        },
      ],
    },
  ])("rejects an invalid field: %s", (fields) => {
    expect(
      getStreamFailureDetails(error({ ...failure(), ...fields })),
    ).toBeUndefined();
  });
});
