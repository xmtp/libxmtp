import * as SQLite from "@xmtp/wa-sqlite";
import initModule from "@xmtp/wa-sqlite/build";

const module = await initModule();
const sqlite3 = SQLite.Factory(module);
const vfs = await OPFSCoopSyncVFS.create("test", module);
sqlite3.vfs_register(vfs, true);

export function sqlite3_result_text(context, value) {
  sqlite3.result_text(context, value);
}

export function sqlite3_result_int(context, value) {
  sqlite3.result_int(context, value);
}

export function sqlite3_result_int64(context, value) {
  sqlite3.result_int64(context, value);
}

export function sqlite3_result_double(context, value) {
  sqlite3.result_double(context, value);
}

export function sqlite3_result_blob(context, value) {
  sqlite3.result_blob(context, value);
}

export function sqlite3_result_null(context) {
  sqlite3.result_null(context);
}

export function establish(database_url) {
  try {
    console.log("Opening database!");
    return sqlite3.open_v2(database_url);
  } catch {
    console.log("establish err");
  }
}

export function batch_execute(database, query) {
  try {
    sqlite3.exec(database, query);
    console.log("Batch exec'ed");
  } catch {
    console.log("exec err");
  }
}
