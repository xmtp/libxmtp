var storage = {};

// Write to local storage
export function writeWrapper(key, value) {
  storage[key] = value;
  console.log('writeWrapper', key, value);
  if (typeof window !== 'undefined') {
    window.localStorage.setItem(key, value);
  }
  return true;
}

export function readWrapper(key) {
  if (typeof window !== 'undefined') {
    return window.localStorage.getItem(key);
  }
  return storage[key];
}
