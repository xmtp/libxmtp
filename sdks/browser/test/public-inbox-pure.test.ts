import {
  XmtpError,
  generateInboxId,
  initPureWasm,
  type PublicIdentity,
} from "@xmtp/browser-sdk/pure";
import {
  afterAll,
  afterEach,
  beforeAll,
  beforeEach,
  expect,
  test,
  vi,
} from "vitest";

const ethereum: PublicIdentity = {
  kind: "ethereum",
  identifier: "0xabcdef0000000000000000000000000000000000",
};
const passkey: PublicIdentity = { kind: "passkey", identifier: "abcdef" };

// The Rust pure identity test checks every fixed vector. These cases keep
// the JavaScript bigint conversion: a nonce above 2^53, the largest u64, and
// the default nonce for the second identity kind.
const vectors = [
  [
    ethereum,
    9_007_199_254_740_993n,
    "7388e86684247cde39d20ef985b6999cb657325d1576c28b74670a5392228913",
  ],
  [
    ethereum,
    18_446_744_073_709_551_615n,
    "00b23df9cd0b16c488b19647e02ee5872a8c7de14d056b937d1cd1f54c4a28fc",
  ],
  [
    passkey,
    0n,
    "e26bbe40a904acb658e0dd48f4031811b662ce4e6238eef5c46f5bb92550713a",
  ],
] as const;

let workerCalls = 0;
let fetchCalls = 0;
let restoreFetch = () => {};

beforeAll(async () => {
  vi.stubGlobal(
    "Worker",
    class ForbiddenWorker {
      constructor() {
        workerCalls++;
        throw new Error("pure inbox calculation created a worker");
      }
    },
  );
  await initPureWasm();
  expect(workerCalls).toBe(0);
});
beforeEach(() => {
  fetchCalls = 0;
  const fetch = vi.spyOn(globalThis, "fetch").mockImplementation(() => {
    fetchCalls++;
    throw new Error("pure inbox calculation made a network request");
  });
  restoreFetch = () => fetch.mockRestore();
});
afterEach(() => {
  restoreFetch();
  expect(workerCalls).toBe(0);
  expect(fetchCalls).toBe(0);
});
afterAll(() => vi.unstubAllGlobals());

test.each(vectors)(
  "pure inbox calculation keeps %o nonce %s and its fixed ID",
  (identity, nonce, expected) => {
    const actual = generateInboxId(identity, nonce);
    expect(actual).toBeTypeOf("string");
    expect(actual).toBe(expected);
  },
);

test("pure inbox calculation defaults to nonce zero", () => {
  const [identity, , expected] = vectors[2];
  expect(generateInboxId(identity)).toBe(expected);
  expect(generateInboxId(identity, undefined)).toBe(expected);
});

function invalidArgument(operation: () => unknown): void {
  let thrown: unknown;
  try {
    operation();
  } catch (error) {
    thrown = error;
  }
  expect(thrown).toBeInstanceOf(XmtpError.InvalidArgument);
  expect((thrown as XmtpError).details).toMatchObject({
    code: "InvalidArgument",
    category: "input",
    retryable: false,
  });
}

test("pure inbox calculation rejects a malformed identity with a typed input error", () =>
  invalidArgument(() =>
    generateInboxId({ kind: "ethereum", identifier: "invalid-address" }),
  ));

test.each([-1n, 18_446_744_073_709_551_616n])(
  "pure inbox calculation rejects nonce %s before unsigned conversion",
  (nonce) => invalidArgument(() => generateInboxId(ethereum, nonce)),
);

test.each([1, Number(9_007_199_254_740_993n), "1", null])(
  "pure inbox calculation rejects non-bigint nonce %s",
  (nonce) =>
    invalidArgument(() => {
      // Check JavaScript callers that do not use the TypeScript declaration.
      // @ts-expect-error nonce must be a bigint.
      generateInboxId(ethereum, nonce);
    }),
);
