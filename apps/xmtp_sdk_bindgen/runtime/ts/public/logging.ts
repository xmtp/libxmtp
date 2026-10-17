import {
  currentProjection,
  liftLogRecord,
  publicError,
  type LogRecord,
} from "../../public-values.gen";
import { setLogSink as setHostLogSink } from "../logging";

/** An app log sink. It receives public log records. */
export interface LogSink {
  log(record: LogRecord): Promise<void>;
}

/**
 * Install or clear the process log sink. Call `initLogging` first; before it,
 * both forms fail with the public `XmtpError.InvalidInput`.
 */
export async function setLogSink(sink?: LogSink): Promise<void> {
  try {
    if (sink === undefined) {
      await setHostLogSink();
      return;
    }
    await setHostLogSink({
      log: (record) => sink.log(liftLogRecord(record, currentProjection())),
    });
  } catch (error) {
    throw publicError(error);
  }
}
