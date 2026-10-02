import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';

const source = readFileSync('apps/xmtp_sdk_bindgen/src/public_projection/policy.rs', 'utf8');
const match = source.match(/const CREDENTIAL_GUARD: &str = r#"([\s\S]*?)"#;/);
assert.ok(match, 'generated credential guard must exist');
const check = new Function('value', match[1]);
const credential = expiresAtSeconds => ({ value: 'Bearer private-test-value', expiresAtSeconds });

test('credential wire guard rejects malformed and out-of-range values without secret text', () => {
  for (const expiry of [NaN, Infinity, 1.5, Number.MAX_SAFE_INTEGER + 1, 1, '1', undefined, null,
    -9223372036854775809n, 9223372036854775808n]) {
    assert.throws(() => check(credential(expiry)), { name: 'TypeError', message: 'invalid credential record' });
  }
  for (const value of [undefined, null, {}, { ...credential(1n), value: undefined },
    { ...credential(1n), value: 42 }, { ...credential(1n), name: null }]) {
    assert.throws(() => check(value), { name: 'TypeError', message: 'invalid credential record' });
  }
});

test('credential wire guard keeps every signed i64 bigint including values beyond Number precision', () => {
  for (const expiry of [-9223372036854775808n, 0n, 1n, BigInt(Number.MAX_SAFE_INTEGER) + 1n, 9223372036854775807n]) {
    const value = credential(expiry);
    check(value);
    assert.equal(value.expiresAtSeconds, expiry);
  }
  check({ ...credential(1n), name: 'authorization' });
});
