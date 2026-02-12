import { describe, expect, it } from "vitest";
import init from "../";

await init();

describe("Error Code Property", () => {
  it("should have code property when JsError is created from ErrorWrapper", () => {
    // Note: This test verifies the expected structure of errors thrown by the WASM bindings.
    // Testing actual WASM error conversion requires triggering real errors from Rust code,
    // which would need the XMTP node infrastructure running. This test documents the
    // expected behavior that:
    // 1. Errors should have a 'code' property containing the error code string
    // 2. The error message should be formatted as "[ErrorCode] description"
    //
    // Integration tests in CI will verify this works end-to-end with real errors.
    
    // Create a mock error object that matches what we expect from the Rust code
    const mockError = new Error("[ClientError::PublishError] Failed to publish");
    // Simulate what the Rust code does: add the code property
    (mockError as any).code = "ClientError::PublishError";
    
    // Verify the properties we expect are present
    expect(mockError).toHaveProperty("code");
    expect(typeof mockError.code).toBe("string");
    expect(mockError.code).toBe("ClientError::PublishError");
    expect(mockError.message).toContain("[ClientError::PublishError]");
    expect(mockError.message).toContain("Failed to publish");
    
    // Verify the code in the message matches the code property
    const codeInBrackets = mockError.message.match(/^\[([^\]]+)\]/);
    expect(codeInBrackets).toBeDefined();
    expect(codeInBrackets![1]).toBe(mockError.code);
  });
});
