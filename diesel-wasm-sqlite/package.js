import * as SQLite from "@xmtp/wa-sqlite";
import SQLiteESMFactory from "./node_modules/@xmtp/wa-sqlite/dist/wa-sqlite.mjs";
import base64Wasm from "./node_modules/@xmtp/wa-sqlite/dist/wa-sqlite.wasm";

function base64Decode(str) {
  const binaryString = typeof atob === "function"
    ? atob(str)
    : Buffer.from(str, "base64").toString("binary");
  const len = binaryString.length;
  const bytes = new Uint8Array(len);
  for (let i = 0; i < len; i++) {
    bytes[i] = binaryString.charCodeAt(i);
  }
  return bytes.buffer;
}

const module = await SQLiteESMFactory({
  "wasmBinary": base64Decode(base64Wasm),
});

// const module = await initWasmModule();
const sqlite3 = SQLite.Factory(module);

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

export async function establish(database_url) {
  try {
    console.log("Opening database!", database_url);
    let db = await sqlite3.open_v2(database_url);
    console.log(db);
    return db;
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
