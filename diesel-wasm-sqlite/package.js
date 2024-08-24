import sqlite3InitModule from "@sqlite.org/sqlite-wasm";

const log = console.log;
const err_log = console.error;

export class SQLiteError extends Error {
  constructor(message, code) {
    super(message);
    this.code = code;
  }
}

export class SQLite {
  #module;
  #sqlite3;
  constructor(module) {
    if (typeof module === "undefined") {
      throw new Error("Cannot be called directly");
    }
    this.sqlite3 = module;
    this.mapStmtToDB = new Map();
  }

  verifyStatement(stmt) {
    if (!mapStmtToDB.has(stmt)) {
      throw new SQLiteError("not a statement", SQLite.SQLITE_MISUSE);
    }
  }

  static async init_module(wasm, opts) {
    return await sqlite3InitModule({
      print: log,
      printErr: err_log,
      ...opts,
    });
  }

  check(code, dbPtr = null, allowed = [this.sqlite3.capi.SQLITE_OK]) {
    if (allowed.includes(code)) return code;
    // dbPtr = dbPtr.pointer;
    const capi = this.sqlite3.capi;
    const message = dbPtr
      ? capi.sqlite3_errmsg(dbPtr)
      : capi.sqlite3_errstr(code);
    throw new SQLiteError(message, code);
  }

  result_js(context, value) {
    this.sqlite3.capi.sqlite3_result_js(context, value);
  }

  result_text(context, value) {
    this.sqlite3.capi.sqlite3_result_text(context, value);
  }

  result_int(context, value) {
    this.sqlite3.capi.sqlite3_result_int(context, value);
  }

  result_int64(context, value) {
    this.sqlite3.capi.sqlite3_result_int64(context, value);
  }

  result_double(context, value) {
    this.sqlite3.capi.sqlite3_result_double(context, value);
  }

  result_blob(context, value) {
    this.sqlite3.capi.sqlite3_result_blob(context, value);
  }

  result_null(context) {
    this.sqlite3.capi.sqlite3_result_null(context);
  }

  bind(stmt, i, value) {
    const lib = this.sqlite3.capi;
    try {
      switch (typeof value) {
        case "number":
          if (value === (value | 0)) {
            return lib.sqlite3_bind_int(stmt, i, value);
          } else {
            return lib.sqlite3_bind_double(stmt, i, value);
          }
        case "string":
          return lib.sqlite3_bind_text(stmt, i, value);
        default:
          if (value instanceof Uint8Array || Array.isArray(value)) {
            return lib.sqlite3_bind_blob(stmt, i, value);
          } else if (value === null) {
            return lib.sqlite3_bind_null(stmt, i);
          } else if (typeof value === "bigint") {
            return lib.sqlite3_bind_int64(stmt, i, value);
          } else if (value === undefined) {
            // Existing binding (or NULL) will be used.
            return lib.SQLITE_NOTICE;
          } else {
            console.warn("unknown binding converted to null", value);
            return lib.bind_null(stmt, i);
          }
      }
      return lib.sqlite3_bind(stmt, i, value);
    } catch (error) {
      console.log(`bind err ${error}`);
      throw error;
    }
  }

  bind_blob(stmt, i, value) {
    try {
      return this.sqlite3.capi.sqlite_bind_blob(stmt, i, value);
    } catch (error) {
      console.log("bind blob error");
      throw error;
    }
  }

  bind_collection(stmt, bindings) {
    try {
      return this.sqlite3.capi.sqlite3_bind_collection(stmt, bindings);
    } catch (error) {
      console.log("bind collection error");
      throw error;
    }
  }

  bind_double(stmt, i, value) {
    try {
      return this.sqlite3.capi.sqlite3_bind_double(stmt, i, value);
    } catch (error) {
      console.log("bind double error");
      throw error;
    }
  }

  bind_int(stmt, i, value) {
    try {
      return this.sqlite3.capi.sqlite3_bind_int(stmt, i, value);
    } catch (error) {
      console.log("bind int error");
      throw error;
    }
  }

  bind_int64(stmt, i, value) {
    try {
      return this.sqlite3.capi.sqlite3_bind_int64(stmt, i, value);
    } catch (error) {
      console.log("bind int644 error");
      throw error;
    }
  }

  bind_null(stmt, i) {
    try {
      return this.sqlite3.capi.sqlite3_bind_null(stmt, i);
    } catch (error) {
      console.log("bind null error");
      throw error;
    }
  }

  bind_parameter_count(stmt) {
    return this.sqlite3.capi.sqlite3_bind_parameter_count(stmt);
  }

  bind_parameter_name(stmt, i) {
    return this.sqlite3.capi.sqlite3_bind_paramater_name(stmt, it);
  }

  bind_text(stmt, i, value) {
    try {
      this.sqlite3.capi.sqlite3_bind_text(stmt, i, value);
    } catch (error) {
      console.log("bind text error");
      throw error;
    }
  }

  reset(stmt) {
    try {
      return this.sqlite3.capi.sqlite3_reset(stmt);
    } catch (error) {
      console.log("reset err");
      throw error;
    }
  }

  value(pValue) {
    this.sqlite3.capi.sqlite3_value_to_js(pValue);
  }

  value_dup(pValue) {
    return this.sqlite3.capi.sqlite3_value_dup(pValue);
  }

  value_blob(pValue) {
    this.sqlite3.capi.sqlite3_value_blob(pValue);
  }

  value_bytes(pValue) {
    this.sqlite3.capi.sqlite3_value_bytes(pValue);
  }

  value_double(pValue) {
    this.sqlite3.capi.sqlite3_value_double(pValue);
  }

  value_int(pValue) {
    this.sqlite3.capi.sqlite3_value_int(pValue);
  }

  value_int64(pValue) {
    this.sqlite3.capi.sqlite3_value_int64(pValue);
  }

  value_text(pValue) {
    this.sqlite3.capi.sqlite3_value_text(pValue);
  }

  value_type(pValue) {
    return this.sqlite3.capi.sqlite3_value_type(pValue);
  }

  open(database_url, iflags) {
    try {
      console.log("Opening database!, ignoring iflags!", database_url);
      const db = new this.sqlite3.oo1.OpfsDb(database_url);
      console.log(`Created persistent database at ${db.filename}`);
      return db;
    } catch (error) {
      console.log("OPFS open error", error);
      throw error;
    }
  }

  exec(db, query) {
    try {
      return db.exec(query, {
        callback: (row) => {
          log(`exec'd ${row}`);
        },
      });
    } catch (error) {
      console.error("exec err");
      throw error;
    }
  }

  finalize(stmt) {
    try {
      return this.sqlite3.capi.sqlite3_finalize(stmt);
    } catch (error) {
      console.error("stmt error");
      throw error;
    }
  }

  changes(db) {
    return this.sqlite3.capi.sqlite3_changes(db);
  }

  clear_bindings(stmt) {
    try {
      return this.sqlite3.capi.sqlite3_clear_bindings(stmt);
    } catch (error) {
      console.error("sqlite3.clear_bindings error");
      throw error;
    }
  }

  close(db) {
    try {
      log("Closing Database!");
      return db.close(db);
    } catch (error) {
      console.error("sqlite3.close error");
      throw error;
    }
  }

  prepare_v3(db, sql, nByte, prepFlags, ppStmt, pzTail) {
    console.log(`Preparing with flags ${prepFlags}`);
    const code = this.sqlite3.capi.sqlite3_prepare_v3(
      db.pointer,
      sql,
      nByte,
      prepFlags,
      ppStmt,
      pzTail,
    );

    if (code !== this.sqlite3.capi.SQLITE_OK) {
      this.check(code);
    }
  }

  into_statement(pStmt) {
    const BindTypes = {
      null: 1,
      number: 2,
      string: 3,
      boolean: 4,
      blob: 5,
    };
    BindTypes["undefined"] == BindTypes.null;
    if (wasm.bigIntEnabled) {
      BindTypes.bigint = BindTypes.number;
    }

    new Stmt(this, pStmt, BindTypes);
  }

  step(stmt) {
    const code = this.sqlite3.capi.sqlite3_step(stmt);
    if (code !== this.sqlite3.capi.SQLITE_OK) {
      const capi = this.sqlite3.capi;
      return this.check(code, null, [capi.SQLITE_ROW, capi.SQLITE_DONE]);
    }
  }

  column(stmt, i) {
    try {
      return this.sqlite3.capi.sqlite3_column_js(stmt, i);
    } catch (error) {
      console.error("Could not convert to JS");
    }
  }

  column_name(stmt, idx) {
    return this.sqlite3.capi.sqlite3_column_name(stmt, idx);
  }

  column_count(stmt) {
    return this.sqlite3.capi.sqlite3_column_count(stmt);
  }

  create_function(
    database,
    functionName,
    nArg,
    textRep,
    pApp,
    xFunc,
    xStep,
    xFinal,
  ) {
    try {
      this.sqlite3.capi.sqlite3_create_function(
        database,
        functionName,
        nArg,
        textRep,
        pApp, // pApp is ignored
        xFunc,
        xStep,
        xFinal,
      );
      console.log("create function");
    } catch (error) {
      console.log("create function err");
      throw error;
    }
  }

  //TODO: At some point need a way to register functions from rust
  //but for just libxmtp this is fine.
  register_diesel_sql_functions(database) {
    console.log("REGISTERING DIESEL");
    try {
      this.sqlite3.capi.sqlite3_create_function(
        database,
        "diesel_manage_updated_at",
        1,
        this.sqlite3.capi.SQLITE_UTF8,
        0,
        async (context, values) => {
          const table_name = this.sqlite3.value_text(values[0]);

          database.exec(
            context,
            `CREATE TRIGGER __diesel_manage_updated_at_${table_name}
            AFTER UPDATE ON ${table_name}
            FOR EACH ROW WHEN
              old.updated_at IS NULL AND
              new.updated_at IS NULL OR
              old.updated_at == new.updated_at
            BEGIN
            UPDATE ${table_name}
            SET updated_at = CURRENT_TIMESTAMP
            WHERE ROWID = new.ROWID;
            END`,
            (row) => {
              log(`------------------------------------`);
              log(`Created trigger for ${table_name}`);
              log(row);
              log(`------------------------------------`);
            },
          );
        },
      );
    } catch (error) {
      console.log("error creating diesel trigger");
      throw error;
    }
  }

  value_free(value) {
    return this.sqlite3.capi.sqlite3_value_free(value);
  }

  /*
  serialize(database, zSchema, size, flags) {
    return this.module._sqlite3_serialize(database, zSchema, size, flags);
  }
  */
}
