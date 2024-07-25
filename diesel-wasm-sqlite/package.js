import * as WasmSQLiteLibrary from "@xmtp/wa-sqlite";
import { OPFSCoopSyncVFS } from "@xmtp/wa-sqlite/vfs/OPFSCoopSync";
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

export class SQLite {
  #module;
  #sqlite3;
  constructor(module) {
    if (typeof module === "undefined") {
      throw new Error("Cannot be called directly");
    }
    this.module = module;
    this.sqlite3 = WasmSQLiteLibrary.Factory(module);
  }

  static async wasm_module() {
    return await SQLiteESMFactory({
      "wasmBinary": base64Decode(base64Wasm),
    });
  }

  static async build() {
    const module = await SQLiteESMFactory({
      "wasmBinary": base64Decode(base64Wasm),
    });
    return new WasmSQLiteLibrary(module);
  }

  result_text(context, value) {
    this.sqlite3.result_text(context, value);
  }

  result_int(context, value) {
    this.sqlite3.result_int(context, value);
  }

  result_int64(context, value) {
    this.sqlite3.result_int64(context, value);
  }

  result_double(context, value) {
    this.sqlite3.result_double(context, value);
  }

  result_blob(context, value) {
    this.sqlite3.result_blob(context, value);
  }

  result_null(context) {
    this.sqlite3.result_null(context);
  }
  
  async open_v2(database_url, iflags) {
    try {
      console.log("Opening database!", database_url);
      const vfs = await OPFSCoopSyncVFS.create(database_url, this.module);
      this.sqlite3.vfs_register(vfs, true);
      let db = await this.sqlite3.open_v2(database_url, iflags);
      return db;
    } catch (error) {
      console.log("openv2 error", error);
      throw error;
    }
  }
  
  async exec(db, query) {
    try {
      return await this.sqlite3.exec(db, query);
    } catch {
      console.log('exec err');
    }
  }

  changes(db) {
    return this.sqlite3.changes(db);
  }

  batch_execute(database, query) {
    try {
      sqlite3.exec(database, query);
      console.log("Batch exec'ed");
    } catch {
      console.log("exec err");
    }
  }
}
