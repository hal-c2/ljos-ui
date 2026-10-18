# Sources:
#   https://doc.qt.io/qt-6/qtquick-localstorage-qmlmodule.html
#   https://doc.qt.io/qt-6/qml-qtquick-localstorage-localstorage.html
# Verified on Qt 6.11.2 (qml6, offscreen).
#
# Qt keeps these databases in the real home directory, so every scenario
# here leaves the same result when it runs again: tables are dropped
# before they are made, and a scenario about a new database names one
# after the time it runs.

Feature: LocalStorage
  LocalStorage.openDatabaseSync opens a SQLite database of the
  application's own, by name. SQL runs inside a transaction: a statement
  that fails throws an error with a code, and a callback that throws
  undoes everything its transaction did.

  @qt-parity
  Rule: A transaction runs SQL and reads its rows back

    @api-QtQuick.LocalStorage.LocalStorage.openDatabaseSync
    Scenario: Rows written in a transaction can be read back
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.LocalStorage
        Item {
          objectName: "o"
          property string got
          Component.onCompleted: {
            const db = LocalStorage.openDatabaseSync("ljos-ls-rows", "1.0", "rows", 1000)
            db.transaction(tx => {
              tx.executeSql("DROP TABLE IF EXISTS t")
              tx.executeSql("CREATE TABLE t(id INTEGER PRIMARY KEY, name TEXT)")
              tx.executeSql("INSERT INTO t(name) VALUES(?)", ["a"])
              const r = tx.executeSql("INSERT INTO t(name) VALUES(?)", "b")
              const rows = tx.executeSql("SELECT id, name FROM t ORDER BY id").rows
              got = [r.rowsAffected, r.insertId, rows.length, rows.item(0).name, rows[1].id].join(",")
            })
          }
        }
        """
      Then the property "got" of "o" is "1,2,2,a,2"

    Scenario: A script reaches LocalStorage through the module it imports under a name
      Given the file "store.js":
        """
        .pragma library
        .import QtQuick.LocalStorage as Sql
        function count() {
          const db = Sql.LocalStorage.openDatabaseSync("ljos-ls-script", "1.0", "script", 1000)
          let n = -1
          db.transaction(tx => {
            tx.executeSql("DROP TABLE IF EXISTS t")
            tx.executeSql("CREATE TABLE t(v INTEGER)")
            tx.executeSql("INSERT INTO t VALUES(7)")
            n = tx.executeSql("SELECT v FROM t").rows.item(0).v
          })
          return n
        }
        """
      And the QML:
        """qml
        import QtQuick
        import "store.js" as Store
        Item {
          objectName: "o"
          property int got: Store.count()
        }
        """
      Then the property "got" of "o" is 7

    Scenario: Named arguments, and the values a row holds
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.LocalStorage
        Item {
          objectName: "o"
          property string got
          Component.onCompleted: {
            const db = LocalStorage.openDatabaseSync("ljos-ls-values", "1.0", "values", 1000)
            db.transaction(tx => {
              tx.executeSql("DROP TABLE IF EXISTS t")
              tx.executeSql("CREATE TABLE t(n TEXT, v INTEGER, b, z)")
              tx.executeSql("INSERT INTO t VALUES(:n, :v, :b, :z)",
                            {":n": "x", ":v": 3, ":b": true, ":z": undefined})
              const row = tx.executeSql("SELECT * FROM t").rows.item(0)
              got = [row.n, typeof row.v, row.v, row.b, row.z === null].join(",")
            })
          }
        }
        """
      Then the property "got" of "o" is "x,number,3,1,true"

    Scenario: A result shows its three values and nothing else
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.LocalStorage
        Item {
          objectName: "o"
          property string got
          Component.onCompleted: {
            const db = LocalStorage.openDatabaseSync("ljos-ls-keys", "1.0", "keys", 1000)
            db.readTransaction(tx => {
              const r = tx.executeSql("SELECT 1 AS one")
              got = [Object.keys(r).join(" "), Object.keys(r.rows).length,
                     r.rows.item(5) === undefined, Object.keys(db).length, r.insertId === ""].join(",")
            })
          }
        }
        """
      Then the property "got" of "o" is "rowsAffected insertId rows,0,true,0,true"

  Rule: What fails throws an error with a code

    @qt-parity
    Scenario Outline: <case> throws code <code>
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.LocalStorage
        Item {
          objectName: "o"
          property string got
          Component.onCompleted: {
            const db = LocalStorage.openDatabaseSync("ljos-ls-errors", "1.0", "errors", 1000)
            try {
              db.<kind>(tx => tx.executeSql(<sql>, <args>))
            } catch (e) {
              got = e.code + ": " + e.message.replace(/"/g, "'")
            }
          }
        }
        """
      Then the property "got" of "o" is "<code>: <message>"

      Examples:
        | case                    | kind            | sql                  | args   | code | message                                          |
        | SQL that does not parse | transaction     | "SELEKT 1"           | []     | 2    | near 'SELEKT': syntax error Unable to execute statement |
        | A table that is not     | transaction     | "SELECT * FROM nope" | []     | 2    | no such table: nope Unable to execute statement  |
        | Too many arguments      | transaction     | "SELECT ?"           | [1, 2] | 2    | Parameter count mismatch                         |
        | A write when reading    | readTransaction | "CREATE TABLE w(x)"  | []     | 6    | Read-only Transaction                            |

    @qt-parity
    Scenario: A transaction needs a callback
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.LocalStorage
        Item {
          objectName: "o"
          property string got
          Component.onCompleted: {
            const db = LocalStorage.openDatabaseSync("ljos-ls-errors", "1.0", "errors", 1000)
            try { db.transaction() } catch (e) { got = e.code + ": " + e.message }
          }
        }
        """
      Then the property "got" of "o" is "1: transaction: missing callback"

    @qt-parity
    Scenario: SQL cannot run once its transaction is over
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.LocalStorage
        Item {
          objectName: "o"
          property string got
          Component.onCompleted: {
            const db = LocalStorage.openDatabaseSync("ljos-ls-errors", "1.0", "errors", 1000)
            let kept
            db.transaction(tx => { kept = tx })
            try { kept.executeSql("SELECT 1") } catch (e) { got = e.code + ": " + e.message }
          }
        }
        """
      Then the property "got" of "o" is "2: executeSql called outside transaction()"

    # The documentation says a callback that throws rolls its transaction
    # back. Qt 6.11.2 keeps every row the callback wrote (here "stop,2"),
    # and the rows before a failed statement too; Ljós does what the
    # documentation says.
    @qt-diverges
    Scenario: A callback that throws undoes its transaction
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.LocalStorage
        Item {
          objectName: "o"
          property string got
          Component.onCompleted: {
            const db = LocalStorage.openDatabaseSync("ljos-ls-undo", "1.0", "undo", 1000)
            db.transaction(tx => {
              tx.executeSql("DROP TABLE IF EXISTS t")
              tx.executeSql("CREATE TABLE t(x)")
              tx.executeSql("INSERT INTO t VALUES('kept')")
            })
            try {
              db.transaction(tx => { tx.executeSql("INSERT INTO t VALUES('lost')"); throw new Error("stop") })
            } catch (e) { got = e.message }
            db.readTransaction(tx => { got += "," + tx.executeSql("SELECT x FROM t").rows.length })
          }
        }
        """
      Then the property "got" of "o" is "stop,1"

  @qt-parity
  Rule: A database has a version

    Scenario: A callback is called once, when the database is made, and leaves it without a version
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.LocalStorage
        Item {
          objectName: "o"
          property string got
          Component.onCompleted: {
            const name = "ljos-ls-new-" + Date.now()
            let calls = 0
            const made = cb => LocalStorage.openDatabaseSync(name, "1.0", "new", 1000, cb)
            const db = made(db => calls++)
            const again = made(db => calls++)
            got = calls + ",[" + db.version + "],[" + again.version + "]"
          }
        }
        """
      Then the property "got" of "o" is "1,[],[]"

    Scenario: changeVersion makes an object of the new version, and the old one keeps its own
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.LocalStorage
        Item {
          objectName: "o"
          property string got
          Component.onCompleted: {
            const name = "ljos-ls-version-" + Date.now()
            const open = v => LocalStorage.openDatabaseSync(name, v, "version", 1000)
            const db = open("1.0")
            const next = db.changeVersion("1.0", "2.0", tx => tx.executeSql("CREATE TABLE t(x)"))
            let refused = ""
            try { open("1.0") } catch (e) { refused = e.message }
            got = [db.version, next.version, open("").version, open("2.0").version, refused].join(",")
          }
        }
        """
      Then the property "got" of "o" is "1.0,2.0,2.0,2.0,SQL: database version mismatch"

    Scenario: changeVersion from a version the object does not have is refused
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.LocalStorage
        Item {
          objectName: "o"
          property string got
          Component.onCompleted: {
            const db = LocalStorage.openDatabaseSync("ljos-ls-refused", "1.0", "refused", 1000)
            try { db.changeVersion("0.9", "2.0") } catch (e) { got = e.code + ": " + e.message }
          }
        }
        """
      Then the property "got" of "o" is "3: Version mismatch: expected 0.9, found 1.0"

  Rule: A database on disk is kept the way SQLite is told to keep it

    # Qt 6.11.2 leaves SQLite as it comes: a journal that is deleted after
    # each transaction, and every commit waits for the disk four times
    # (journal twice, its directory, the file). A busy disk then holds the
    # UI thread for seconds. Ljós keeps a write-ahead log and does not wait
    # for the disk on commit, as Android keeps its databases: a committed
    # transaction is read by every later one and by the next run, and is
    # lost only to a crash of the whole machine, never to one of the
    # application. Qt reports "delete,2".
    @qt-diverges
    Scenario: A database on disk keeps a write-ahead log and commits without waiting for the disk
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.LocalStorage
        Item {
          objectName: "o"
          property string got
          Component.onCompleted: {
            const db = LocalStorage.openDatabaseSync("ljos-ls-journal", "1.0", "journal", 1000)
            db.transaction(tx => {
              const mode = tx.executeSql("PRAGMA journal_mode").rows.item(0).journal_mode
              const sync = tx.executeSql("PRAGMA synchronous").rows.item(0).synchronous
              got = mode + "," + sync
            })
          }
        }
        """
      Then the property "got" of "o" is "wal,1"

    # SQLite enforces a foreign key only when told to, and Qt does not tell
    # it to.
    @qt-parity
    Scenario: A foreign key is not enforced
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.LocalStorage
        Item {
          objectName: "o"
          property string got
          Component.onCompleted: {
            const db = LocalStorage.openDatabaseSync("ljos-ls-keys", "1.0", "keys", 1000)
            db.transaction(tx => {
              tx.executeSql("DROP TABLE IF EXISTS c")
              tx.executeSql("DROP TABLE IF EXISTS p")
              tx.executeSql("CREATE TABLE p(id INTEGER PRIMARY KEY)")
              tx.executeSql("CREATE TABLE c(p INTEGER REFERENCES p(id))")
              tx.executeSql("INSERT INTO c VALUES(5)")
              got = tx.executeSql("PRAGMA foreign_keys").rows.item(0).foreign_keys + ","
                  + tx.executeSql("SELECT count(*) AS n FROM c").rows.item(0).n
            })
          }
        }
        """
      Then the property "got" of "o" is "0,1"
