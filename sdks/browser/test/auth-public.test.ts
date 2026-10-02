import { Client } from "@xmtp/browser-sdk";
import { expect, test, vi } from "vitest";

import { backend, create, options, signer } from "./helpers";

const credential = () => ({
  value: "Bearer sdk-auth-test-key-00000000000000000000",
  expiresAtSeconds: BigInt(Math.floor(Date.now() / 1000) + 3600),
});

test("standalone queries keep credential sources separate at one endpoint", async () => {
  const identity = await signer().identity();
  const first = vi.fn(async () => credential());
  const second = vi.fn(async () => credential());
  await Client.canMessage([identity], {
    ...backend,
    credentials: { credential: first },
  });
  expect(first).toHaveBeenCalledOnce();
  expect(second).not.toHaveBeenCalled();
  await Client.canMessage([identity], {
    ...backend,
    credentials: { credential: second },
  });
  expect(first).toHaveBeenCalledOnce();
  expect(second).toHaveBeenCalledOnce();
});

test("public configuration fetch needs no credential but client identity reads use its source", async () => {
  const callback = vi.fn(async () => credential());
  const configured = { ...backend, credentials: { credential: callback } };
  await Client.fetchServerConfiguration(configured);
  expect(callback).not.toHaveBeenCalled();
  const client = await create(signer(), { backend: configured });
  expect(callback).toHaveBeenCalled();
  expect(await client.canMessage([client.identity])).toBeInstanceOf(Map);
});

test("the real worker keeps private credential failures out of public errors", async () => {
  const callback = vi.fn(async () => {
    throw new Error("private refresh response");
  });
  const creation = Client.create(signer(), {
    ...options,
    backend: { ...backend, credentials: { credential: callback } },
  });
  await expect(creation).rejects.not.toThrow("private refresh response");
  expect(callback).toHaveBeenCalled();
});

test("an authenticated backend refreshes a rejected credential through the worker", async ({
  skip,
}) => {
  if (!(await Client.fetchServerConfiguration(backend)).auth.enabled) skip();
  let calls = 0;
  const callback = vi.fn(async () => ({
    ...credential(),
    value:
      ++calls === 1
        ? "Bearer wrong-sdk-auth-key-00000000000000000000"
        : credential().value,
  }));
  const client = await create(signer(), {
    backend: { ...backend, credentials: { credential: callback } },
  });
  expect(await client.isRegistered()).toBe(true);
  expect(calls).toBeGreaterThan(1);
});
