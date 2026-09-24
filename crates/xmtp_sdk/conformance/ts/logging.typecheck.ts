import type { LogSink } from "../../../../apps/xmtp_sdk_bindgen/runtime/ts/logging";

export function readLogFields(record: Parameters<LogSink["log"]>[0]): void {
  const level: number = record.level;
  const fields: Map<string, string> = record.fields;
  const droppedRecords: bigint = record.droppedRecords;
  void [level, fields, droppedRecords];
}
