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

  bind(stmt, i, value) {
    try {
      return this.sqlite3.bind(stmt, i, value);
    } catch (error) {
      console.log("bind err");
      throw error;
    }
  }

  bind_blob(stmt, i, value) {
    try {
      return this.sqlite3.bind_blob(stmt, i, value);
    } catch (error) {
      console.log("bind blob error");
      throw error;
    }
  }

  bind_collection(stmt, bindings) {
    try {
      return this.sqlite3.bind_collection(stmt, bindings);
    } catch (error) {
      console.log("bind collection error");
      throw error;
    }
  }

  bind_double(stmt, i, value) {
    try {
      return this.sqlite3.bind_double(stmt, i, value);
    } catch (error) {
      console.log("bind double error");
      throw error;
    }
  }

  bind_int(stmt, i, value) {
    try {
      return this.sqlite3.bind_int(stmt, i, value);
    } catch (error) {
      console.log("bind int error");
      throw error;
    }
  }

  bind_int64(stmt, i, value) {
    try {
      return this.sqlite3.bind_int64(stmt, i, value);
    } catch (error) {
      console.log("bind int644 error");
      throw error;
    }
  }

  bind_null(stmt, i) {
    try {
      return this.sqlite3.bind_null(stmt, i);
    } catch (error) {
      console.log("bind null error");
      throw error;
    }
  }

  bind_parameter_count(stmt) {
    return this.sqlite3.bind_parameter_count(stmt);
  }

  bind_parameter_name(stmt, i) {
    return this.sqlite3.bind_paramater_name(stmt, it);
  }

  bind_text(stmt, i, value) {
    try {
      this.sqlite3.bind_text(stmt, i, value);
    } catch (error) {
      console.log("bind text error");
      throw error;
    }
  }

  async reset(stmt) {
    try {
      return await this.sqlite3.reset(stmt);
    } catch (error) {
      console.log("reset err");
      throw error;
    }
  }

  value(pValue) {
    this.sqlite3.value(pValue);
  }

  value_dup(pValue) {
    return this.module._sqlite3_value_dup(pValue);
  }

  value_blob(pValue) {
    this.sqlite3.value_blob(pValue);
  }

  value_bytes(pValue) {
    this.sqlite3.value_bytes(pValue);
  }

  value_double(pValue) {
    this.sqlite3.value_double(pValue);
  }

  value_int(pValue) {
    this.sqlite3.value_int(pValue);
  }

  value_int64(pValue) {
    this.sqlite3.value_int64(pValue);
  }

  value_text(pValue) {
    this.sqlite3.value_text(pValue);
  }

  value_type(pValue) {
    this.sqlite3.value_type(pValue);
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
      console.log("exec err");
    }
  }

  finalize(stmt) {
    try {
      return this.sqlite3.finalize(stmt);
    } catch (error) {
      console.log("stmt error");
    }
  }

  changes(db) {
    return this.sqlite3.changes(db);
  }

  clear_bindings(stmt) {
    return this.sqlite3.clear_bindings(stmt);
  }

  async close(db) {
    try {
      return this.sqlite3.close(db);
    } catch (error) {
      console.log("sqlite3.close error");
      throw error;
    }
  }

  column(stmt, i) {
    return this.sqlite3.column(stmt, i);
  }

  async prepare(database, sql, options) {
    try {
      return await this.sqlite3.statements(database, sql, options);
    } catch (error) {
      console.log("sqlite prepare error");
      throw error;
    }
  }

  async step(stmt) {
    try {
      return await this.sqlite3.step(stmt);
    } catch (error) {
      console.log("sqlite step error");
      throw error;
    }
  }

  column_name(stmt, idx) {
    return this.sqlite3.column_name(stmt, idx);
  }

  column_count(stmt) {
    return this.sqlite3.column_count(stmt);
  }

  batch_execute(database, query) {
    try {
      return sqlite3.exec(database, query);
      console.log("Batch exec'ed");
    } catch {
      console.log("exec err");
    }
  }

  create_function(database, functionName, nArg, textRep, xFunc, xStep, xFinal) {
    try {
      sqlite.create_function(
        database,
        functionName,
        nArg,
        textRep,
        0,
        xFunc,
        xStep,
        xFinal,
      );
      console.log("create function");
    } catch {
      console.log("create function err");
    }
  }
}
