import { loggingInWorker } from "../package-session.gen.js";
import { setLogSink as setWorkerLogSink } from "../proxy.gen.js";
import { logSinkSetter } from "../runtime/bridge/main/log-sink.js";
import type { LogRecord } from "../xmtp_sdk.js";

export interface LogSink {
  log(record: LogRecord): Promise<void>;
}

/** Install or clear the package worker's asynchronous sink. */
const update = logSinkSetter<LogSink>(loggingInWorker, setWorkerLogSink);

export function setLogSink(sink?: LogSink): Promise<void> {
  return update(sink);
}
