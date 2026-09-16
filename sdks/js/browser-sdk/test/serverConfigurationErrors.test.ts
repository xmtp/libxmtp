import { describe, expect, it } from "vitest";
import {
  AuthRequiredError,
  BackendMismatchError,
  ChainNotAcceptedError,
  ClientVersionTooOldError,
  ConfigurationInvalidError,
  ConfigurationUnavailableError,
  getErrorCode,
  ServerConfigurationError,
  toServerConfigurationError,
} from "@/utils/errors";

// Spec 006 CFG-083: each code is its own type, not a message string.
const codes = [
  ["ClientError::ConfigurationUnavailable", ConfigurationUnavailableError],
  ["ClientError::ConfigurationInvalid", ConfigurationInvalidError],
  ["ClientError::BackendMismatch", BackendMismatchError],
  ["ClientError::ClientVersionTooOld", ClientVersionTooOldError],
  ["ClientError::AuthRequired", AuthRequiredError],
  ["ClientError::ChainNotAccepted", ChainNotAcceptedError],
] as const;

describe("server configuration errors", () => {
  it("should map every configuration error code to its own type", () => {
    for (const [code, ErrorClass] of codes) {
      // Only the message survives a worker error transfer.
      const error = toServerConfigurationError(
        new Error(`[${code}] something went wrong`),
      );
      expect(error).toBeInstanceOf(ErrorClass);
      expect(error).toBeInstanceOf(ServerConfigurationError);
      expect(error?.code).toBe(code);
      expect(error?.message).toBe("something went wrong");
    }
  });

  it("should read the code property the bindings set", () => {
    const original = Object.assign(new Error("no credential configured"), {
      code: "ClientError::AuthRequired",
    });
    expect(getErrorCode(original)).toBe("ClientError::AuthRequired");
    const error = toServerConfigurationError(original);
    expect(error).toBeInstanceOf(AuthRequiredError);
    expect(error?.cause).toBe(original);
  });

  it("should leave other errors untyped", () => {
    expect(toServerConfigurationError(new Error("group not found"))).toBe(
      undefined,
    );
    expect(
      toServerConfigurationError(new Error("[GroupError::Other] boom")),
    ).toBe(undefined);
    expect(getErrorCode(undefined)).toBe(undefined);
  });
});
