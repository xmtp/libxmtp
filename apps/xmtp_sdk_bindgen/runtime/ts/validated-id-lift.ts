const validatedIdLift =
  /^Failed to convert arg '[^']+':\nLifting custom type `xmtp_sdk::ids::(?:InboxId|InstallationId|ConversationId|MessageId)` from FFI type `alloc::string::String` failed(?: at [^\n]+)?\n\nCaused by:\n    invalid argument: ErrorDetails \{ code: "InvalidArgument", category: Input, retryable: false, message: "(invalid lowercase hex ID|inbox ID is empty)" \}$/;

export function validatedIdLiftMessage(error: unknown): string | undefined {
  if (!(error instanceof Error)) return undefined;
  return validatedIdLift.exec(error.message)?.[1];
}
