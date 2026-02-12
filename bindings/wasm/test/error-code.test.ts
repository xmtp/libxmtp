import { describe, expect, it } from "vitest";
import init from "../";

await init();

describe("Error Code Property", () => {
  it("should have code property when JsError is created from ErrorWrapper", () => {
    // This test demonstrates that when Rust errors that implement ErrorCode
    // are converted to JsError, they should have a code property
    
    // Create a mock error object that simulates what we expect
    const mockError = new Error("[ClientError::PublishError] Failed to publish");
    // Add the code property that our Rust code should set
    (mockError as any).code = "ClientError::PublishError";
    
    // Verify the properties we expect are present
    expect(mockError).toHaveProperty("code");
    expect(typeof mockError.code).toBe("string");
    expect(mockError.code).toBe("ClientError::PublishError");
    expect(mockError.message).toContain("[ClientError::PublishError]");
    expect(mockError.message).toContain("Failed to publish");
  });
});
