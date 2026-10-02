import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';

const source = readFileSync('apps/xmtp_sdk_bindgen/src/callback_results.rs', 'utf8');
const match = source.match(/const REJECTION_HELPER: &str = r#"([\s\S]*?)"#;/);
assert.ok(match, 'generated callback rejection helper must exist');
const body = match[1].slice(match[1].indexOf('{') + 1, match[1].lastIndexOf('}'));
const normalize = new Function('error', 'isErrorType', body);
class TypedFailure extends Error {}
const isTyped = value => value instanceof TypedFailure;

// Use the pinned handler's message read before its typed error check.
async function completion(value) {
  let completed = 0;
  let code;
  await (async () => { try { throw value; } catch (error) { throw normalize(error, isTyped); } })()
    .then(() => { completed += 1; }, error => {
      const message = error.message ? error.message : error.toString();
      code = isTyped(error) ? 'typed' : 'unexpected';
      assert.equal(typeof message, 'string');
      completed += 1;
    });
  assert.equal(completed, 1);
  return code;
}

test('callback rejection settles null, undefined and normal errors without private text', async () => {
  for (const value of [null, undefined, new Error('private-test-text')]) {
    assert.equal(await completion(value), 'unexpected');
    assert.equal(normalize(value, isTyped).message, 'Foreign callback failed');
  }
});

test('void callback rejection keeps typed errors and permits the next call', async () => {
  assert.equal(await completion(undefined), 'unexpected');
  const value = new TypedFailure('typed');
  assert.equal(normalize(value, isTyped), value);
  assert.equal(await completion(value), 'typed');
  let calls = 0;
  await (async () => { calls += 1; })();
  assert.equal(calls, 1);
});
