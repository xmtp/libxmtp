import {
  XmtpError,
  generateInboxId,
  type PublicIdentity,
} from "@xmtp/node-sdk";
import { expect, test } from "vitest";

const ethereum: PublicIdentity = {
  kind: "ethereum",
  identifier: "0xabcdef0000000000000000000000000000000000",
};
const passkey: PublicIdentity = { kind: "passkey", identifier: "abcdef" };

// These fixed vectors also occur in the Rust pure identity test.
const vectors = [
  [
    ethereum,
    0n,
    "139a684d70154ab320b846179e5219b6e2d192048577779b230763a85a28365d",
  ],
  [
    ethereum,
    1n,
    "f020cf771dabaf2610250b5f00076215a8f1da8649ba46cf5ba2d00df6ce5279",
  ],
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
  [
    passkey,
    1n,
    "ac9f830ae6cf2299ba293dd4cec3be0d87a88e6a8fbfe5015de6fffd11d79b6e",
  ],
  [
    passkey,
    9_007_199_254_740_993n,
    "ef91da01728a1d16593d300a7a699d6a7831c43e00916a6b199f8244f207637d",
  ],
  [
    passkey,
    18_446_744_073_709_551_615n,
    "469fa9bb87114e117a27305350728cb2a4f85fdae9b5ccaef3b702f879dd9ae4",
  ],
] as const;

test.each(vectors)(
  "synchronous inbox calculation keeps %o nonce %s and its fixed ID",
  (identity, nonce, expected) => {
    const actual = generateInboxId(identity, nonce);
    expect(actual).toBeTypeOf("string");
    expect(actual).toBe(expected);
  },
);

test.each([vectors[0], vectors[4]])(
  "synchronous inbox calculation defaults %o to nonce zero",
  (identity, _nonce, expected) => {
    expect(generateInboxId(identity)).toBe(expected);
    expect(generateInboxId(identity, undefined)).toBe(expected);
  },
);

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

test.each([-1n, 18_446_744_073_709_551_616n])(
  "synchronous inbox calculation rejects nonce %s before unsigned conversion",
  (nonce) => invalidArgument(() => generateInboxId(ethereum, nonce)),
);

test.each([1, Number(9_007_199_254_740_993n), "1", null])(
  "synchronous inbox calculation rejects non-bigint nonce %s",
  (nonce) =>
    invalidArgument(() => {
      // Check JavaScript callers that do not use the TypeScript declaration.
      // @ts-expect-error nonce must be a bigint.
      generateInboxId(ethereum, nonce);
    }),
);
