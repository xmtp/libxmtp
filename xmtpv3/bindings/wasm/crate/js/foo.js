var storage = {};

// Write to local storage
export function writeWrapper(key, value) {
  storage[key] = value;
  console.log('writeWrapper', key, value);
  return true;
}

export function readWrapper(key) {
  return storage[key];
}
