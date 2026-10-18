import assert from "node:assert/strict";
import { test } from "node:test";

import {
  connectToAddress,
  normalizeBackendUrl,
  parseBackendUrl,
} from "./backendUrl.ts";
import { SessionCopy, SessionError } from "./sessionError.ts";

const message = (input: string): string => {
  try {
    normalizeBackendUrl(input);
  } catch (error) {
    assert.ok(error instanceof SessionError);
    return error.message;
  }
  assert.fail(`${input} was accepted`);
};

test("loopback hosts allow private attachment destinations (BackendURLTests.loopback)", () => {
  for (const url of [
    "http://localhost:5050",
    "http://127.0.0.1",
    "http://[::1]:5050",
    "https://localhost",
  ]) {
    assert.equal(normalizeBackendUrl(url).allowPrivateNetwork, true, url);
  }
});

test("rejects what iOS rejects (BackendURLTests.rejects)", () => {
  for (const url of [
    "http://example.com",
    "https://example.com:0",
    "https://example.com:65536",
    "https://user@example.com",
    "https://example.com?q=x",
    "https://example.com#x",
    "ftp://localhost",
    "",
  ]) {
    assert.equal(parseBackendUrl(url), undefined, url);
  }
});

test("each rejection has the iOS copy", () => {
  assert.equal(message(""), SessionCopy.invalidUrl);
  assert.equal(message("ftp://localhost"), SessionCopy.invalidUrl);
  assert.equal(message("example.com"), SessionCopy.invalidUrl);
  assert.equal(message("https://"), SessionCopy.invalidUrl);
  assert.equal(message("https://exa mple.com"), SessionCopy.invalidUrl);
  assert.equal(message("https://example.com:abc"), SessionCopy.invalidUrl);
  assert.equal(message("https://user@example.com"), SessionCopy.urlParts);
  assert.equal(message("https://@example.com"), SessionCopy.urlParts);
  assert.equal(message("https://example.com?q=x"), SessionCopy.urlParts);
  assert.equal(message("https://example.com/?"), SessionCopy.urlParts);
  assert.equal(message("https://example.com#x"), SessionCopy.urlParts);
  assert.equal(message("https://example.com:0"), SessionCopy.urlParts);
  assert.equal(message("https://example.com:65536"), SessionCopy.urlParts);
  assert.equal(message("https://example.com:"), SessionCopy.urlParts);
  assert.equal(message("https://example.com:/api"), SessionCopy.urlParts);
  assert.equal(message("http://example.com"), SessionCopy.remoteHttp);
  // Only the iOS loopback names count; other local addresses need HTTPS.
  assert.equal(message("http://127.0.0.2"), SessionCopy.remoteHttp);
  assert.equal(message("http://0.0.0.0"), SessionCopy.remoteHttp);
  assert.equal(message("http://10.0.2.2:5050"), SessionCopy.remoteHttp);
});

test("equivalent endpoints share a profile key (equivalentEndpointsShareProfileKey)", () => {
  for (const [a, b] of [
    ["HTTPS://EXAMPLE.COM:443", "https://example.com"],
    ["HTTP://LOCALHOST:80/", "http://localhost"],
    ["https://EXAMPLE.COM:443/API/v1///", "https://example.com/API/v1"],
    ["http://[::1]:80", "http://[::1]"],
  ] as const) {
    assert.deepEqual(normalizeBackendUrl(a), normalizeBackendUrl(b), a);
    assert.equal(normalizeBackendUrl(a).value, b);
  }
});

test("distinct endpoints keep separate keys (distinctEndpointsKeepSeparateProfileKeys)", () => {
  for (const [a, b] of [
    ["https://example.com:444", "https://example.com"],
    ["http://localhost:5050", "http://localhost"],
    ["https://example.com/API", "https://example.com/api"],
  ] as const) {
    assert.notEqual(
      normalizeBackendUrl(a).value,
      normalizeBackendUrl(b).value,
      a,
    );
  }
});

test("trims and normalizes a remote URL (remoteAndNormalization)", () => {
  assert.deepEqual(normalizeBackendUrl(" https://example.com/// "), {
    value: "https://example.com",
    allowPrivateNetwork: false,
  });
  assert.equal(
    normalizeBackendUrl("https://example.com//a").value,
    "https://example.com//a",
  );
  assert.equal(
    normalizeBackendUrl("http://127.0.0.1:5150").value,
    "http://127.0.0.1:5150",
  );
});

test("Connect to shows the default port", () => {
  assert.equal(
    connectToAddress("https://example.com"),
    "https://example.com:443",
  );
  assert.equal(
    connectToAddress("HTTP://LOCALHOST/api/"),
    "http://localhost:80/api",
  );
  assert.equal(
    connectToAddress("http://127.0.0.1:5150"),
    "http://127.0.0.1:5150",
  );
  assert.equal(connectToAddress("http://[::1]"), "http://[::1]:80");
  assert.equal(connectToAddress("http://example.com"), undefined);
});
