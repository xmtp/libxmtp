import * as raw from "../xmtp_sdk";
import type { LogRecord } from "../xmtp_sdk";

export interface LogSink {
  log(record: LogRecord): Promise<void>;
}

/** Install or clear the single asynchronous process sink. */
export async function setLogSink(sink?: LogSink): Promise<void> {
  await raw.setLogSink(
    sink === undefined
      ? undefined
      : {
          async log(record: LogRecord): Promise<void> {
            if (!raw.sdkLogSinkHandoff()) return;
            try {
              await sink.log(record);
            } catch {
              throw new raw.LogSinkError.Failed({
                reason: "log callback failed",
              });
            }
          },
        },
  );
}
