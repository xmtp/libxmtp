import type { AuthCallback, Credential } from "@/types/options";

/** Keep callback failures and invalid credentials out of error messages. */
export const readCredential = async (
  callback: AuthCallback,
): Promise<Credential> => {
  try {
    const credential = await callback();
    if (
      typeof credential.value !== "string" ||
      (credential.name !== undefined && typeof credential.name !== "string") ||
      !Number.isSafeInteger(credential.expiresAtSeconds)
    ) {
      throw new Error();
    }
    return {
      name: credential.name,
      value: credential.value,
      expiresAtSeconds: credential.expiresAtSeconds,
    };
  } catch {
    throw new Error("auth callback failed");
  }
};
