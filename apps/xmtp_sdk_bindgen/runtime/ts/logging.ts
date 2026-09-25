import * as raw from "../xmtp_sdk";
import type { LogRecord } from "../xmtp_sdk";

export interface LogSink {
  log(record: LogRecord): void;
}

// JavaScript callbacks run on the bounded sink thread. Rust calls never wait for them.
export function setLogSink(sink?: LogSink): void {
  const queued = (
    raw as unknown as {
      setLogSinkQueued?: (sink: unknown) => void;
      clearLogSink?: () => void;
    }
  ).setLogSinkQueued;
  if (sink === undefined) {
    (raw as unknown as { clearLogSink?: () => void }).clearLogSink?.();
    return;
  }
  if (queued === undefined) throw new Error("log sinks need a native host");
  queued(sink);
}
