import { setPackageLogSink } from "../package-session.gen.js";
import type { LogRecord } from "../xmtp_sdk.js";

export interface LogSink {
  log(record: LogRecord): Promise<void>;
}

/** Install or clear the package worker's asynchronous sink. */
export function setLogSink(sink?: LogSink): Promise<void> {
  return setPackageLogSink(sink);
}
