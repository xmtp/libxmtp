import { loggingInWorker } from "../package-session.gen.js";
import { setLogSink as setWorkerLogSink } from "../proxy.gen.js";
import type { LogRecord } from "../xmtp_sdk.js";

export interface LogSink {
  log(record: LogRecord): Promise<void>;
}

/** Install or clear the package worker's asynchronous sink. */
export async function setLogSink(sink?: LogSink): Promise<void> {
  await loggingInWorker(async (session) => {
    session.callbacks.clearLogSink();
    await setWorkerLogSink(session, sink);
  });
}
