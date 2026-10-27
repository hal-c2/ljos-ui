//! The `QtQuick.LocalStorage` module for Ljós: `LocalStorage`, the
//! singleton whose `openDatabaseSync` opens a SQLite database of the
//! application's own by name, and the database objects it gives.
//!
//! This is an opt-in module crate. The `ljos` command has it in `run`,
//! `render` and `test` (in memory for the last two); any other host
//! registers it like any native module, and may say where the databases
//! are kept:
//!
//! ```ignore
//! ljos_localstorage::register(&engine);
//! ljos_localstorage::set_storage(&engine, ljos_localstorage::Storage::Memory);
//! ```
//!
//! ## Where a database is
//!
//! By default where Qt keeps it, so a Qt build and a Ljós build of one
//! application share their data: `QML/OfflineStorage/Databases` under the
//! application's data directory (`StandardPaths.AppDataLocation`), a
//! `<md5 of the name>.sqlite` file and an `.ini` beside it that holds the
//! name, description, estimated size and version. [`Storage::Memory`]
//! keeps every database in memory for as long as the engine runs, which is
//! what a tool that must leave no file behind wants.
//!
//! ## The surface (Qt 6.11.2 as probed on `qml6`)
//!
//! The scenarios are in `features/platform/local-storage.feature`.
//! `openDatabaseSync(name, version, description, estimatedSize,
//! callback)`; on the database, `version`, `transaction`,
//! `readTransaction` and `changeVersion`; in a transaction,
//! `executeSql(sql, args)` with positional (an array or one value) or
//! named (`{":id": 1}`) arguments, whose result has `rowsAffected`,
//! `insertId` and `rows` (`length`, `item(i)`, `rows[i]`). Errors carry
//! Qt's codes and messages. One deliberate difference: a callback that
//! throws rolls its transaction back, as the documentation says; Qt 6.11.2
//! keeps what the callback wrote. A version mismatch in `openDatabaseSync`
//! throws Qt's message without its `code`, which an error a native method
//! throws cannot carry.

mod md5;

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

use ljos_engine::{Cx, Engine, ObjectId, QmlObject, Value, Version, qml_impl, qml_singleton};
use ljos_platform::{Location, Platform};
use rusqlite::Connection;
use rusqlite::types::ValueRef;

/// Registers `LocalStorage` (`QtQuick.LocalStorage`) on `engine`.
pub fn register(engine: &Engine) {
    engine.register_singleton::<LocalStorage>(MODULE, Version::new(2, 0));
}

/// Where the databases of an engine are kept.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Storage {
    /// Where Qt keeps them, under the application's data directory.
    #[default]
    Standard,
    /// In this directory instead.
    Directory(PathBuf),
    /// In memory, for as long as the engine runs.
    Memory,
}

/// Says where the databases `engine` opens from now on are kept. A
/// database already open stays where it is.
pub fn set_storage(engine: &Engine, storage: Storage) {
    *engine.extension::<Store>().storage.borrow_mut() = storage;
}

const MODULE: &str = "QtQuick.LocalStorage";

/// Qt's error codes, as `SQLException` names them.
mod code {
    pub const UNKNOWN: i32 = 1;
    pub const DATABASE: i32 = 2;
    pub const SYNTAX: i32 = 6;
}

#[derive(Default)]
struct Store {
    storage: RefCell<Storage>,
    open: RefCell<HashMap<String, Rc<Database>>>,
}

/// One database, open for as long as the engine runs: every object
/// `openDatabaseSync` gives for its name shares it.
struct Database {
    connection: Connection,
    /// The `.ini` that keeps what Qt keeps beside the file; none in memory.
    ini: Option<PathBuf>,
    meta: RefCell<Meta>,
    /// The native object the scripts of its database objects call.
    handle: ObjectId,
}

#[derive(Clone, Debug, Default)]
struct Meta {
    name: String,
    description: String,
    size: i64,
    version: String,
}

impl Meta {
    fn ini_text(&self) -> String {
        format!(
            "[General]\nDescription={}\nDriver=QSQLITE\nEstimatedSize={}\nName={}\nVersion={}\n",
            self.description, self.size, self.name, self.version
        )
    }
}

impl Database {
    fn write_meta(&self) {
        if let Some(ini) = &self.ini {
            // A version that cannot be written is still the version of
            // this run, as the file is the only thing that failed.
            let _ = std::fs::write(ini, self.meta.borrow().ini_text());
        }
    }
}

/// What an `.ini` Qt wrote says the version is.
fn stored_version(text: &str) -> String {
    let value = text
        .lines()
        .find_map(|l| l.trim().strip_prefix("Version="))
        .unwrap_or_default()
        .trim();
    value
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .unwrap_or(value)
        .to_owned()
}

/// The directory databases go in, or `None` to keep them in memory.
fn directory(engine: &Engine) -> Option<PathBuf> {
    let storage = engine.extension::<Store>().storage.borrow().clone();
    match storage {
        Storage::Memory => None,
        Storage::Directory(dir) => Some(dir),
        Storage::Standard => {
            let platform = Platform::of(engine);
            let (org, app) = platform.application(engine);
            let env = platform.env();
            let dir = Location::AppData
                .writable(&env, &org, &app)
                .join("QML/OfflineStorage/Databases");
            env.real(&dir).ok()
        }
    }
}

/// A write-ahead log, synced at checkpoints rather than on every commit.
/// SQLite's own default waits for the disk four times per commit, and on
/// a busy disk that holds the UI thread for seconds. What a QML app can
/// see stays the same: a committed transaction is read by every later
/// one and by the next run, and only a crash of the whole machine can
/// take the last commits back, never the database. A file system that
/// cannot keep a log leaves SQLite's defaults in place.
fn keep_on_disk(connection: &Connection) {
    let mode: String = connection
        .query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))
        .unwrap_or_default();
    if mode.eq_ignore_ascii_case("wal") {
        let _ = connection.execute_batch("PRAGMA synchronous = NORMAL");
    }
}

/// The database `meta.name` names, opened if it is not yet, and whether
/// it was made just now.
fn open(cx: &Cx, meta: Meta) -> Result<(Rc<Database>, bool), String> {
    let store = cx.engine.extension::<Store>();
    if let Some(db) = store.open.borrow().get(&meta.name) {
        return Ok((Rc::clone(db), false));
    }
    let fail = |e: &dyn std::fmt::Display| format!("SQL: cannot open database: {e}");
    let (connection, ini, stored) = match directory(cx.engine) {
        None => (
            Connection::open_in_memory().map_err(|e| fail(&e))?,
            None,
            None,
        ),
        Some(dir) => {
            std::fs::create_dir_all(&dir).map_err(|e| fail(&e))?;
            let base = dir.join(md5::hex(meta.name.as_bytes()));
            let ini = base.with_extension("ini");
            let stored = std::fs::read_to_string(&ini)
                .ok()
                .map(|t| stored_version(&t));
            let connection =
                Connection::open(base.with_extension("sqlite")).map_err(|e| fail(&e))?;
            keep_on_disk(&connection);
            (connection, Some(ini), stored)
        }
    };
    // rusqlite's SQLite enforces foreign keys from the start; the one a
    // QML app is written against leaves that to the app.
    let _ = connection.execute_batch("PRAGMA foreign_keys = OFF");
    let name = meta.name.clone();
    let handle = cx
        .engine
        .create_native_incomplete(SqlDatabase { name: name.clone() }, Some(cx.this))
        .map_err(|e| fail(&e))?;
    let made = stored.is_none();
    let meta = Meta {
        version: stored.unwrap_or_else(|| meta.version.clone()),
        ..meta
    };
    let db = Rc::new(Database {
        connection,
        ini,
        meta: RefCell::new(meta),
        handle,
    });
    if made {
        db.write_meta();
    }
    store.open.borrow_mut().insert(name, Rc::clone(&db));
    Ok((db, made))
}

/// `LocalStorage` (QtQuick.LocalStorage).
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick.LocalStorage", name = "LocalStorage")]
#[qml_singleton]
pub struct LocalStorage {}

fn given(v: &Value) -> bool {
    !matches!(v, Value::Undefined | Value::Null)
}

#[qml_impl]
impl LocalStorage {
    #[qml(invokable, name = "openDatabaseSync")]
    fn open_database_sync(
        cx: &Cx,
        name: Value,
        version: Value,
        description: Value,
        estimated_size: Value,
        callback: Value,
    ) -> Result<Value, String> {
        let text = |v: &Value| {
            if given(v) {
                v.to_js_string().to_string()
            } else {
                String::new()
            }
        };
        let requested = text(&version);
        let with_callback = given(&callback);
        let meta = Meta {
            name: text(&name),
            description: text(&description),
            size: estimated_size.to_number() as i64,
            // A database made with a callback has no version until the
            // callback gives it one.
            version: if with_callback {
                String::new()
            } else {
                requested.clone()
            },
        };
        let (db, made) = open(cx, meta)?;
        let stored = db.meta.borrow().version.clone();
        let version = if stored.is_empty() || requested.is_empty() || requested == stored {
            stored
        } else {
            return Err("SQL: database version mismatch".to_owned());
        };
        let factory = cx
            .engine
            .eval_js(DATABASE_JS)
            .map_err(|e| format!("LocalStorage: {e}"))?;
        let made = Value::Bool(made && with_callback);
        cx.engine
            .call_js(
                &factory,
                &[
                    Value::Object(db.handle),
                    Value::from(version.as_str()),
                    callback,
                    made,
                ],
            )
            .ok_or_else(|| "LocalStorage: the database object was not made".to_owned())
    }
}

/// What the scripts of one database's objects call: SQL, the edges of a
/// transaction and the version. Not a type QML can name.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick.LocalStorage", name = "LocalStorageDatabase")]
pub struct SqlDatabase {
    name: String,
}

/// The database `cx.this` is a handle of. None only when `this` is no
/// handle: a database stays open while the engine runs, and every SQL
/// call then fails with "database is not open".
fn database(cx: &Cx) -> Option<Rc<Database>> {
    let store = cx.engine.extension::<Store>();
    let open = store.open.borrow();
    // The name is looked up where it lies, not copied for every call.
    cx.engine
        .with(cx.this, |h: &SqlDatabase| open.get(&h.name).cloned())
        .ok()
        .flatten()
}

/// `["error", code, message]`, which the script throws.
fn error(code: i32, message: &str) -> Value {
    Value::list(vec![
        Value::from("error"),
        Value::Int(code),
        Value::from(message),
    ])
}

fn sql_message(e: &rusqlite::Error) -> String {
    let said = match e {
        rusqlite::Error::SqliteFailure(_, Some(m)) => m.clone(),
        rusqlite::Error::SqlInputError { msg, .. } => msg.clone(),
        e => e.to_string(),
    };
    format!("{said} Unable to execute statement")
}

fn sql_value(v: &Value) -> rusqlite::types::Value {
    use rusqlite::types::Value as Sql;
    match v {
        Value::Undefined | Value::Null => Sql::Null,
        Value::Bool(b) => Sql::Integer(i64::from(*b)),
        Value::Int(i) => Sql::Integer(i64::from(*i)),
        Value::Real(r) if r.fract() == 0.0 && r.abs() < 9.0e15 => Sql::Integer(*r as i64),
        Value::Real(r) => Sql::Real(*r),
        v => Sql::Text(v.to_js_string().to_string()),
    }
}

fn row_value(v: ValueRef<'_>) -> Value {
    match v {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(i) => i32::try_from(i).map_or(Value::Real(i as f64), Value::Int),
        ValueRef::Real(r) => Value::Real(r),
        ValueRef::Text(t) | ValueRef::Blob(t) => Value::from(String::from_utf8_lossy(t).as_ref()),
    }
}

fn items(v: &Value) -> Vec<Value> {
    match v {
        Value::List(l) => l.to_vec(),
        _ => Vec::new(),
    }
}

/// Runs one statement: `["ok", columns, rows, rowsAffected, insertId]`.
fn execute(db: &Database, sql: &str, names: &[Value], values: &[Value], read_only: bool) -> Value {
    let conn = &db.connection;
    let mut stmt = match conn.prepare(sql) {
        Ok(s) => s,
        Err(e) => return error(code::DATABASE, &sql_message(&e)),
    };
    if read_only && !stmt.readonly() {
        return error(code::SYNTAX, "Read-only Transaction");
    }
    let mismatch = || error(code::DATABASE, "Parameter count mismatch");
    if values.len() > stmt.parameter_count() {
        return mismatch();
    }
    for (i, value) in values.iter().enumerate() {
        let index = match names.get(i) {
            Some(name) => match stmt.parameter_index(&name.to_js_string()) {
                Ok(Some(index)) => index,
                _ => return mismatch(),
            },
            None => i + 1,
        };
        if let Err(e) = stmt.raw_bind_parameter(index, sql_value(value)) {
            return error(code::DATABASE, &sql_message(&e));
        }
    }
    let columns: Vec<Value> = stmt.column_names().into_iter().map(Value::from).collect();
    let width = columns.len();
    let mut rows = Vec::new();
    let mut query = stmt.raw_query();
    loop {
        match query.next() {
            Ok(Some(row)) => rows.push(Value::list(
                (0..width)
                    .map(|i| row.get_ref(i).map_or(Value::Null, row_value))
                    .collect(),
            )),
            Ok(None) => break,
            Err(e) => return error(code::DATABASE, &sql_message(&e)),
        }
    }
    let id = conn.last_insert_rowid();
    Value::list(vec![
        Value::from("ok"),
        Value::list(columns),
        Value::list(rows),
        Value::Real(conn.changes() as f64),
        Value::from(
            if id == 0 {
                String::new()
            } else {
                id.to_string()
            }
            .as_str(),
        ),
    ])
}

// A savepoint, not BEGIN: a transaction a callback opens inside another
// one nests instead of failing.
const BEGIN: &str = "SAVEPOINT ljos_transaction";
const COMMIT: &str = "RELEASE ljos_transaction";
const ROLLBACK: &str = "ROLLBACK TO ljos_transaction; RELEASE ljos_transaction";

fn batch(cx: &Cx, sql: &str) -> Value {
    let Some(db) = database(cx) else {
        return error(code::UNKNOWN, "SQL: database is not open");
    };
    match db.connection.execute_batch(sql) {
        Ok(()) => Value::list(vec![Value::from("ok")]),
        Err(e) => error(code::DATABASE, &sql_message(&e)),
    }
}

#[qml_impl]
impl SqlDatabase {
    #[qml(invokable)]
    fn begin(cx: &Cx) -> Value {
        batch(cx, BEGIN)
    }

    #[qml(invokable)]
    fn commit(cx: &Cx) -> Value {
        batch(cx, COMMIT)
    }

    #[qml(invokable)]
    fn rollback(cx: &Cx) -> Value {
        batch(cx, ROLLBACK)
    }

    #[qml(invokable)]
    fn exec(cx: &Cx, sql: Value, names: Value, values: Value, read_only: Value) -> Value {
        let Some(db) = database(cx) else {
            return error(code::UNKNOWN, "SQL: database is not open");
        };
        execute(
            &db,
            &sql.to_js_string(),
            &items(&names),
            &items(&values),
            read_only.to_boolean(),
        )
    }

    #[qml(invokable, name = "setVersion")]
    fn set_version(cx: &Cx, version: Value) {
        if let Some(db) = database(cx) {
            db.meta.borrow_mut().version = version.to_js_string().to_string();
            db.write_meta();
        }
    }
}

/// Makes the object `openDatabaseSync` returns. What a script sees of a
/// database, a transaction and a result is plain script objects whose
/// members do not enumerate, as Qt's do not; only SQL runs natively.
const DATABASE_JS: &str = r#"(function (handle, version, callback, made) {
  function fail(code, message) {
    var e = new Error(message);
    e.code = code;
    return e;
  }
  function checked(r) {
    if (r[0] !== "ok") throw fail(r[1], r[2]);
    return r;
  }
  function pad(n, w) {
    var s = String(n);
    while (s.length < w) s = "0" + s;
    return s;
  }
  function value(v) {
    if (v === undefined || v === null) return null;
    if (typeof v === "boolean") return v ? 1 : 0;
    if (v instanceof Date) {
      return pad(v.getFullYear(), 4) + "-" + pad(v.getMonth() + 1, 2) + "-" + pad(v.getDate(), 2)
        + "T" + pad(v.getHours(), 2) + ":" + pad(v.getMinutes(), 2) + ":" + pad(v.getSeconds(), 2)
        + "." + pad(v.getMilliseconds(), 3);
    }
    if (typeof v === "object" || typeof v === "function") return "";
    return v;
  }
  function bindings(a) {
    if (a === undefined) return [[], []];
    if (Array.isArray(a)) return [[], a.map(value)];
    if (a !== null && typeof a === "object" && !(a instanceof Date)) {
      var keys = Object.keys(a);
      return [keys, keys.map(function (k) { return value(a[k]); })];
    }
    return [[], [value(a)]];
  }
  function result(r) {
    var columns = r[1];
    var items = r[2].map(function (values) {
      var row = {};
      for (var i = 0; i < columns.length; i++) row[columns[i]] = values[i];
      return row;
    });
    var rows = {};
    Object.defineProperty(rows, "length", { value: items.length });
    Object.defineProperty(rows, "item", { value: function (i) { return items[i]; } });
    for (var i = 0; i < items.length; i++) Object.defineProperty(rows, i, { value: items[i] });
    return { rowsAffected: r[3], insertId: r[4], rows: rows };
  }
  function run(callback, readOnly, what) {
    if (typeof callback !== "function") throw fail(1, what + ": missing callback");
    var live = true;
    var tx = {};
    Object.defineProperty(tx, "executeSql", { value: function (sql, args) {
      if (!live) throw fail(2, "executeSql called outside transaction()");
      var b = bindings(args);
      return result(checked(handle.exec(String(sql), b[0], b[1], readOnly)));
    } });
    checked(handle.begin());
    try {
      callback(tx);
    } catch (e) {
      live = false;
      handle.rollback();
      throw e;
    }
    live = false;
    checked(handle.commit());
  }
  function database(version) {
    var db = {};
    Object.defineProperties(db, {
      version: { value: version },
      transaction: { value: function (cb) { run(cb, false, "transaction"); } },
      readTransaction: { value: function (cb) { run(cb, true, "readTransaction"); } },
      changeVersion: { value: function (from, to, cb) {
        if (String(from) !== version) {
          throw fail(3, "Version mismatch: expected " + from + ", found " + version);
        }
        handle.setVersion(String(to));
        if (cb !== undefined && cb !== null) run(cb, false, "changeVersion");
        return database(String(to));
      } }
    });
    return db;
  }
  var db = database(version);
  if (made && typeof callback === "function") callback(db);
  return db;
})"#;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use ljos_engine::{Engine, Value};
    use ljos_platform::{PathEnv, Platform};

    use super::{LocalStorage, Meta, md5, register, stored_version};

    fn engine(root: &Path) -> Engine {
        let e = Engine::new();
        register(&e);
        let platform = Platform::of(&e);
        platform.set_env(PathEnv::sandbox("/home/user", root));
        platform.set_application("Example", "Notes");
        e
    }

    /// Calls a script function with the `LocalStorage` singleton.
    fn run(e: &Engine, function: &str) -> String {
        let storage = e.singleton::<LocalStorage>().expect("the singleton");
        let f = e.eval_js(function).expect("evaluated");
        e.call_js(&f, &[Value::Object(storage)])
            .expect("ran")
            .to_js_string()
            .to_string()
    }

    #[test]
    fn a_database_is_where_qt_keeps_it_and_outlives_its_engine() {
        let dir = tempfile::tempdir().expect("temp dir");
        let wrote = run(
            &engine(dir.path()),
            "(function (ls) {
               var db = ls.openDatabaseSync('notes', '1.0', 'Notes', 1000);
               db.transaction(function (tx) {
                 tx.executeSql('CREATE TABLE t(x)');
                 tx.executeSql('INSERT INTO t VALUES(?)', ['kept']);
               });
               return db.version;
             })",
        );
        assert_eq!(wrote, "1.0");
        let base = dir
            .path()
            .join("home/user/.local/share/Example/Notes/QML/OfflineStorage/Databases")
            .join(md5::hex(b"notes"));
        let ini = std::fs::read_to_string(base.with_extension("ini")).expect("the .ini");
        assert!(
            ini.contains("Name=notes\n") && ini.contains("Version=1.0\n"),
            "{ini}"
        );
        assert!(base.with_extension("sqlite").is_file());

        let read = run(
            &engine(dir.path()),
            "(function (ls) {
               var db = ls.openDatabaseSync('notes', '', '', 0), got;
               db.readTransaction(function (tx) {
                 got = db.version + ':' + tx.executeSql('SELECT x FROM t').rows.item(0).x;
               });
               return got;
             })",
        );
        assert_eq!(read, "1.0:kept");
    }

    /// The log is not synced on commit, so a second engine on the same
    /// file must still read the commit while the first holds it open.
    #[test]
    fn a_commit_is_read_by_another_engine_while_the_first_keeps_it_open() {
        let dir = tempfile::tempdir().expect("temp dir");
        let first = engine(dir.path());
        let wrote = run(
            &first,
            "(function (ls) {
               var db = ls.openDatabaseSync('log', '1.0', 'Log', 1000), mode;
               db.transaction(function (tx) {
                 mode = tx.executeSql('PRAGMA journal_mode').rows.item(0).journal_mode;
                 tx.executeSql('CREATE TABLE t(x)');
                 tx.executeSql('INSERT INTO t VALUES(?)', ['seen']);
               });
               return mode;
             })",
        );
        assert_eq!(wrote, "wal");
        let read = run(
            &engine(dir.path()),
            "(function (ls) {
               var db = ls.openDatabaseSync('log', '', '', 0), got;
               db.readTransaction(function (tx) {
                 got = tx.executeSql('SELECT x FROM t').rows.item(0).x;
               });
               return got;
             })",
        );
        assert_eq!(read, "seen");
        drop(first);
    }

    #[test]
    fn the_version_is_read_from_what_qt_wrote() {
        let meta = Meta {
            name: "todo".into(),
            description: "Tasks".into(),
            size: 1000,
            version: "1.0".into(),
        };
        assert_eq!(stored_version(&meta.ini_text()), "1.0");
        assert_eq!(stored_version("[General]\nVersion=\"2.0\"\n"), "2.0");
        assert_eq!(stored_version("[General]\nVersion=\n"), "");
    }
}
