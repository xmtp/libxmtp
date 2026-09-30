import {
  currentProjection,
  liftLogRecord,
  publicError,
  type LogRecord,
} from "../../public-values.gen";
import { setLogSink as setHostLogSink } from "../logging";

/** An app log sink. It receives public log records. */
export interface LogSink {
  log(record: LogRecord): void;
}

/**
 * Install or clear the process log sink. Call `initLogging` first; before it,
 * both forms fail with the public `XmtpError.InvalidInput`.
 */
export function setLogSink(sink?: LogSink): void {
  try {
    if (sink === undefined) {
      setHostLogSink();
      return;
    }
    setHostLogSink({
      log: (record) => sink.log(liftLogRecord(record, currentProjection())),
    });
  } catch (error) {
    throw publicError(error);
  }
}
