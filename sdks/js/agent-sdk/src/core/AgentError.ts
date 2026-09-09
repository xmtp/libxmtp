/** Error with a numeric Agent SDK code and optional original cause. */
export class AgentError extends Error {
  #code: number;

  /** Create an error with a stable code for middleware and logs. */
  constructor(code: number, message: string, cause?: unknown) {
    super(message, { cause });
    this.#code = code;
  }

  /** Return the numeric error code. */
  get code() {
    return this.#code;
  }
}

/** Error raised when an Agent SDK stream fails. */
export class AgentStreamingError extends AgentError {}
