import * as SecureStore from "expo-secure-store";

/**
 * Device-only secret storage: the iOS Keychain or the Android Keystore.
 * Readable after the first unlock (background work needs that), never synced
 * or restored to another device. Every call uses the same service, so reads
 * find what writes stored.
 */
const OPTIONS: SecureStore.SecureStoreOptions = {
  keychainService: "org.xmtp.example.reactnative",
  keychainAccessible: SecureStore.AFTER_FIRST_UNLOCK_THIS_DEVICE_ONLY,
};

/** expo-secure-store keys allow only letters, digits, `.`, `-` and `_`. */
const KEY = /^[\w.-]+$/u;

function checked(key: string): string {
  if (!KEY.test(key)) throw new Error(`Invalid secure store key: ${key}`);
  return key;
}

/** The stored text, or undefined. */
export async function secureGet(key: string): Promise<string | undefined> {
  return (await SecureStore.getItemAsync(checked(key), OPTIONS)) ?? undefined;
}

export async function secureSet(key: string, value: string): Promise<void> {
  await SecureStore.setItemAsync(checked(key), value, OPTIONS);
}

/** Removes the key. A missing key is not an error. */
export async function secureDelete(key: string): Promise<void> {
  await SecureStore.deleteItemAsync(checked(key), OPTIONS);
}
