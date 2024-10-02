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
  constructor(sqlite3) {
    if (typeof sqlite3 === "undefined") {
      throw new Error(
        "`sqliteObject` must be defined before calling constructor",
      );
    }
    this.sqlite3 = sqlite3;
  }

  static async init_module(opts) {
    return await sqlite3InitModule({
      print: log,
      printErr: err_log,
      ...opts,
    });
  }

  version() {
    return this.sqlite3.version;
  }

  filename(db, name) {
    return this.sqlite3.capi.sqlite3_db_filename(db, name);
  }

  extended_errcode(connection) {
    return this.sqlite3.capi.sqlite3_extended_errcode(connection);
  }

  errstr(code) {
    return this.sqlite3.capi.sqlite3_errstr(code);
  }

  errmsg(connection) {
    return this.sqlite3.capi.sqlite3_errmsg(connection);
  }

  result_js(context, value) {
    return this.sqlite3.capi.sqlite3_result_js(context, value);
  }

  result_text(context, value) {
    return this.sqlite3.capi.sqlite3_result_text(context, value);
  }

  result_int(context, value) {
    return this.sqlite3.capi.sqlite3_result_int(context, value);
  }

  result_int64(context, value) {
    return this.sqlite3.capi.sqlite3_result_int64(context, value);
  }

  result_double(context, value) {
    return this.sqlite3.capi.sqlite3_result_double(context, value);
  }

  result_blob(context, value) {
    return this.sqlite3.capi.sqlite3_result_blob(context, value);
  }

  result_null(context) {
    return this.sqlite3.capi.sqlite3_result_null(context);
  }

  bind_blob(stmt, i, value, len, flags) {
    return this.sqlite3.capi.sqlite3_bind_blob(stmt, i, value, len, flags);
  }

  bind_text(stmt, i, value, len, flags) {
    return this.sqlite3.capi.sqlite3_bind_text(stmt, i, value, len, flags);
  }

  bind_double(stmt, i, value) {
    return this.sqlite3.capi.sqlite3_bind_double(stmt, i, value);
  }

  bind_int(stmt, i, value) {
    return this.sqlite3.capi.sqlite3_bind_int(stmt, i, value);
  }

  bind_int64(stmt, i, value) {
    return this.sqlite3.capi.sqlite3_bind_int64(stmt, i, value);
  }

  bind_null(stmt, i) {
    this.sqlite3.capi.sqlite3_bind_null(stmt, i);
    /// There's no way bind_null can fail.
    return this.sqlite3.capi.SQLITE_OK;
  }

  bind_parameter_count(stmt) {
    return this.sqlite3.capi.sqlite3_bind_parameter_count(stmt);
  }

  bind_parameter_name(stmt, i) {
    return this.sqlite3.capi.sqlite3_bind_paramater_name(stmt, it);
  }

  value_dup(pValue) {
    return this.sqlite3.capi.sqlite3_value_dup(pValue);
  }

  value_blob(pValue) {
    return this.sqlite3.capi.sqlite3_value_blob(pValue);
  }

  value_bytes(pValue) {
    return this.sqlite3.capi.sqlite3_value_bytes(pValue);
  }

  value_double(pValue) {
    return this.sqlite3.capi.sqlite3_value_double(pValue);
  }

  value_int(pValue) {
    return this.sqlite3.capi.sqlite3_value_int(pValue);
  }

  value_int64(pValue) {
    return this.sqlite3.capi.sqlite3_value_int64(pValue);
  }

  value_text(pValue) {
    return this.sqlite3.capi.sqlite3_value_text(pValue);
  }

  value_type(pValue) {
    return this.sqlite3.capi.sqlite3_value_type(pValue);
  }

  open(database_url, iflags) {
    try {
      return new this.sqlite3.oo1.OpfsDb(database_url);
    } catch (error) {
      console.log("OPFS open error", error);
      throw error;
    }
  }

  exec(db, query) {
    try {
      return db.exec(query, {
        callback: (row) => {},
      });
    } catch (error) {
      throw error;
    }
  }

  finalize(stmt) {
    return this.sqlite3.capi.sqlite3_finalize(stmt);
  }

  changes(db) {
    return this.sqlite3.capi.sqlite3_changes(db);
  }

  clear_bindings(stmt) {
    return this.sqlite3.capi.sqlite3_clear_bindings(stmt);
  }

  reset(stmt) {
    return this.sqlite3.capi.sqlite3_reset(stmt);
  }

  close(db) {
    return this.sqlite3.capi.sqlite3_close_v2(db.pointer);
  }

  db_handle(stmt) {
    return this.sqlite3.capi.sqlite3_db_handle(stmt);
  }

  prepare_v3(db, sql, nByte, prepFlags, ppStmt, pzTail) {
    return this.sqlite3.capi.sqlite3_prepare_v3(
      db.pointer,
      sql,
      nByte,
      prepFlags,
      ppStmt,
      pzTail,
    );
  }

  step(stmt) {
    return this.sqlite3.capi.sqlite3_step(stmt);
  }

  column_value(stmt, i) {
    return this.sqlite3.capi.sqlite3_column_value(stmt, i);
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
  //but for now this is fine.
  register_diesel_sql_functions(database) {
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

  sqlite3_serialize(database, z_schema, p_size, m_flags) {
    try {
      return this.sqlite3.capi.sqlite3_serialize(
        database,
        z_schema,
        p_size,
        m_flags,
      );
    } catch (error) {
      console.log("error serializing");
      throw error;
    }
  }

  sqlite3_deserialize(
    database,
    z_schema,
    p_data,
    sz_database,
    sz_buffer,
    m_flags,
  ) {
    try {
      return this.sqlite3.capi.sqlite3_deserialize(
        database,
        z_schema,
        p_data,
        sz_database,
        sz_buffer,
        m_flags,
      );
    } catch (error) {
      console.log("error deserializing");
      throw error;
    }
  }

  sqlite3_free(_database, arg1) {
    return this.sqlite3.capi.sqlite3_free(arg1);
  }
}
