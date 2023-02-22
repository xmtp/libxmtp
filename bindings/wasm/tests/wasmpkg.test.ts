import { expect, it } from "vitest";
import { XmtpApi } from "..";

it("can instantiate", async () => {
  const a = await XmtpApi.initialize();
  expect(a).toBeDefined();

  // Make sure we can call it again
  const b = await XmtpApi.initialize();
  expect(b).toBeDefined();
});

it("can set private_key_bundle", async () => {
  const a = await XmtpApi.initialize();
  // Base64 decode the private key bundle
  const bundle = Buffer.from("EpYDCsgBCMDw7ZjWtOygFxIiCiAvph+Hg/Gk9G1g2EoW1ZDlWVH1nCkn6uRL7GBG3iNophqXAQpPCMDw7ZjWtOygFxpDCkEEeH4w/gK5HMaKu51aec/jiosmqDduIaEA67V7Lbox1cPhz9SIEi6sY/6jVQQXeIjKxzsZSVrM0LXCXjc0VkRmxhJEEkIKQNSujk9ApV5gIKltm0CFhLLuN3Xt2fjkKZBoUH/mswjTaUMTc3qZZzde3ZKMfkNVZYqns4Sn0sgopXzpjQGgjyUSyAEIwPXBtNa07KAXEiIKIOekWIyRJCelxqX+mR8i76KuDO2QV3e42nv8CxJQL0DXGpcBCk8IwPXBtNa07KAXGkMKQQTIePKpkAHxREbLbXfn6XCOwx9YqQWmqLuTHAnqRNj1q5xDLpbgkiyAORFZmVOK8iVq3dT/PWm6WMasPrqdzD7iEkQKQgpAqIj/yKx2wn8VjeWV6wm/neNDEQ6282p3CeJsPDKS56B11Nqc5Y5vUPKcrC1nB2dqBkwvop0fU49Yx4k0CB2evQ==", "base64");

  const res = await a.setPrivateKeyBundle(bundle);
  expect(res).toBe(true);
});

