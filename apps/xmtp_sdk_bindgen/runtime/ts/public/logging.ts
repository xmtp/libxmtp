import {
  currentProjection,
  liftLogRecord,
  type LogRecord,
} from "../../public-values.gen";
import { setLogSink as setHostLogSink } from "../logging";

/** An app log sink. It receives public log records. */
export interface LogSink {
  log(record: LogRecord): void;
}

/** Install or clear the process log sink. */
export function setLogSink(sink?: LogSink): void {
  if (sink === undefined) {
    setHostLogSink();
    return;
  }
  setHostLogSink({
    log: (record) => sink.log(liftLogRecord(record, currentProjection())),
  });
}
