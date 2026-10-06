import { randomUUID } from "node:crypto";

import {
  Client,
  type ClientOptions,
  type ContentCodec,
  type ContentTypeId,
  type EncodedContent,
  type PublicIdentity,
  type Signer,
} from "@xmtp/node-sdk";
import { toBytes } from "viem";
import { generatePrivateKey, privateKeyToAccount } from "viem/accounts";
import { onTestFinished } from "vitest";

export const sleep = (ms: number) =>
  new Promise((resolve) => setTimeout(resolve, ms));
export const createUser = (key = generatePrivateKey()) => {
  const account = privateKeyToAccount(key);
  return { key, account };
};
export type User = ReturnType<typeof createUser>;
export const createIdentifier = (user: User): PublicIdentity => ({
  kind: "ethereum",
  identifier: user.account.address.toLowerCase(),
});
export const createSigner = (user = createUser()) => {
  const identifier = createIdentifier(user);
  const signer: Signer = {
    identity: async () => identifier,
    kind: async () => ({ kind: "eoa" }),
    sign: async (request) => ({
      kind: "ecdsa",
      value: toBytes(await user.account.signMessage({ message: request.text })),
    }),
  };
  return { address: identifier.identifier, identifier, signer, user };
};
export function clientOptions(
  options: Partial<ClientOptions> = {},
): ClientOptions {
  const backend = options.backend ?? { url: process.env.XMTP_BACKEND_URL! };
  return {
    deviceSync: false,
    storage: {
      location: {
        dbPath: `./test-${randomUUID()}.db3`,
        attachmentsDir: `./test-attachments-${randomUUID()}`,
      },
    },
    ...options,
    backend,
  };
}
/**
 * End `resource` when the current test finishes, also after a failed step.
 * Ending an ended client or stream does nothing.
 */
export function endAfterTest<T extends { end(): Promise<void> }>(
  resource: T,
): T {
  onTestFinished(() => resource.end());
  return resource;
}
export const createRegisteredClient = (
  signer: Signer,
  options?: Partial<ClientOptions>,
) => Client.create(signer, clientOptions(options));
export const createClient = (
  signer: Signer,
  options?: Partial<ClientOptions>,
) =>
  Client.create(
    signer,
    clientOptions({
      ...options,
      registration: { ...options?.registration, auto: false },
    }),
  );
export const buildClient = (
  identity: PublicIdentity,
  options?: Partial<ClientOptions>,
) => Client.build(identity, clientOptions(options));
export const ContentTypeTest: ContentTypeId = {
  authorityId: "xmtp.org",
  typeId: "test",
  versionMajor: 1,
  versionMinor: 0,
};
export class TestCodec implements ContentCodec<Record<string, string>> {
  type = ContentTypeTest;
  encode(value: Record<string, string>): EncodedContent {
    return {
      type: this.type,
      parameters: new Map(),
      content: new TextEncoder().encode(JSON.stringify(value)),
    };
  }
  decode(value: EncodedContent): Record<string, string> {
    return JSON.parse(new TextDecoder().decode(value.content));
  }
  shouldPush() {
    return false;
  }
}
export class DecodeFailureCodec implements ContentCodec<string> {
  type = {
    authorityId: "test",
    typeId: "decode-failure",
    versionMajor: 1,
    versionMinor: 0,
  };
  encode(value: string): EncodedContent {
    return {
      type: this.type,
      parameters: new Map(),
      content: new TextEncoder().encode(value),
    };
  }
  decode(_value: EncodedContent): string {
    throw new Error("Decode failure");
  }
  shouldPush() {
    return false;
  }
}
