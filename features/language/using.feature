# Sources:
#   https://github.com/tc39/proposal-explicit-resource-management (stage 4 since 2026-05-19)
#   https://tc39.es/proposal-explicit-resource-management/ (DisposeResources, DisposableStack,
#     AsyncDisposableStack, SuppressedError)
#   docs/DIALECT.md §1 and §4.22
# Qt 6.11's engine has no `using`: there is no oracle.
# A reported column counts the characters of the line as it is in the file, and a file given here is
# written with the eight spaces it is indented by: the first visible column is 9.

@ljos-only @headless
Feature: Disposing of a resource when its block ends, with using
  A `using` declaration names a value that is disposed of when the block
  around it ends, however it ends: its `[Symbol.dispose]()` method is
  called, the last declared first. `await using` awaits
  `[Symbol.asyncDispose]()`. A `DisposableStack` holds several, and hands
  them on with `move()`. In a file written in Ljós's API, the checker
  holds what a `using` declaration is given to be disposable, or
  `undefined`.

  Rule: A value a using declaration names is disposed of when its block ends

    Scenario: Values are disposed of last first, whether the block ends or throws
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string said: ""
          Component.onCompleted: {
            const log = [];
            const resource = (name) => ({ [Symbol.dispose]() { log.push(name); } });
            {
              using a = resource("a");
              using b = resource("b"), none = null, nothing = undefined;
              log.push("body");
            }
            try {
              using c = resource("c");
              throw new Error("thrown");
            } catch (e) {
              log.push(e.message);
            }
            said = log.join(" ");
          }
        }
        """
      Then the property "said" of "root" is "body b a c thrown"

    Scenario: A DisposableStack disposes of what it holds last first, and move hands it on
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string said: ""
          Component.onCompleted: {
            const log = []; let kept;
            const resource = (name) => ({ [Symbol.dispose]() { log.push(name); } });
            {
              using stack = new DisposableStack();
              const r = stack.use(resource("used"));
              stack.adopt(7, (v) => log.push("adopted " + v));
              stack.defer(() => log.push("deferred"));
              log.push(typeof r[Symbol.dispose]);
            }
            {
              using stack = new DisposableStack();
              stack.defer(() => log.push("moved"));
              kept = stack.move();
              log.push("disposed " + stack.disposed);
            }
            log.push("kept " + kept.disposed);
            kept.dispose();
            said = log.join(", ");
          }
        }
        """
      Then the property "said" of "root" is "function, deferred, adopted 7, used, disposed true, kept false, moved"

    Scenario: An error while disposing is kept with the one it follows, as a SuppressedError
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string said: ""
          Component.onCompleted: {
            try {
              using a = { [Symbol.dispose]() { throw new Error("dispose"); } };
              throw new Error("body");
            } catch (e) {
              said = e.name + ": " + e.error.message + " over " + e.suppressed.message;
            }
          }
        }
        """
      Then the property "said" of "root" is "SuppressedError: dispose over body"

    Scenario: A value that is not disposable is a TypeError where it is declared
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string said: ""
          Component.onCompleted: {
            const log = [];
            try {
              using a = { [Symbol.dispose]() { log.push("a"); } };
              using b = { close() {} };
              log.push("body");
            } catch (e) {
              log.push(e.name);
            }
            said = log.join(" ");
          }
        }
        """
      Then the property "said" of "root" is "a TypeError"

    Scenario: await using awaits an asynchronous disposal, and takes a synchronous one too
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string said: ""
          Component.onCompleted: {
            const log = [];
            (async () => {
              {
                await using a = { async [Symbol.asyncDispose]() { log.push("async"); } };
                await using b = { [Symbol.dispose]() { log.push("sync"); } };
                log.push("body");
              }
              const stack = new AsyncDisposableStack();
              stack.defer(async () => { log.push("deferred"); });
              await stack.disposeAsync();
              said = log.join(" ");
            })();
          }
        }
        """
      When 10 milliseconds pass
      Then the property "said" of "root" is "body sync async deferred"

  Rule: In Ljós's API, what a using declaration is given is disposable

    Scenario: A disposable object, a DisposableStack and their asynchronous forms are typed
      Given the file "pool.lts":
        """ts
        export function work(): number {
          const opened: string[] = [];
          const open = (name: string) => ({
            name: name,
            [Symbol.dispose](): void { opened.push(name); }
          });
          using first = open("a");
          using nothing = undefined;
          using stack = new DisposableStack();
          const second = stack.use(open("bc"));
          const count = stack.adopt(2, (n: number): void => { opened.push(String(n)); });
          stack.defer((): void => { opened.push("deferred"); });
          const moved: DisposableStack = stack.move();
          return first.name.length + second.name.length + count + (moved.disposed ? 1 : 0);
        }
        export async function later(): Promise<void> {
          await using it = { async [Symbol.asyncDispose](): Promise<void> {} };
          await using plain = { [Symbol.dispose](): void {} };
          await using stack = new AsyncDisposableStack();
          stack.defer(async (): Promise<void> => {});
          await stack.disposeAsync();
        }
        """
      And the file "tsconfig.json":
        """json
        { "extends": "./.ljos/tsconfig.base.json" }
        """
      When the files are checked
      Then the check reports:
        """
        1 files, 0 errors, 0 warnings
        """

    Scenario: A using declaration given what has no dispose method is an error at the value
      Given the file "Pool.qml":
        """qml
        import Ljos.Ui
        Item {
          function work(): void {
            using n = 1;
            using plain = { close(): void {} };
            using later = { async [Symbol.asyncDispose](): Promise<void> {} };
            const stack = new DisposableStack();
            stack.use({ name: "x" });
          }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Pool.qml:4:23: error: `using` disposes of what it is given, and this is a number with no `[Symbol.dispose]()` method: give an object with one, or `undefined` [ts-assign-type]
        Pool.qml:5:27: error: `using` disposes of what it is given, and this is an object with no `[Symbol.dispose]()` method: give an object with one, or `undefined` [ts-assign-type]
        Pool.qml:6:27: error: `using` disposes of what it is given at once, and this is an object disposed of asynchronously: write `await using`, or give it a `[Symbol.dispose]()` method [ts-assign-type]
        Pool.qml:8:23: error: `use` disposes of what it is given, and this is an object with no `[Symbol.dispose]()` method: give an object with one, or `undefined` [ts-assign-type]
        1 files, 4 errors, 0 warnings
        ljos: 4 errors
        """

    Scenario: Symbol.dispose is the key of a method in an object literal and nothing else
      Given the file "Pool.qml":
        """qml
        import Ljos.Ui
        Item {
          function work(): void {
            const r = { [Symbol.dispose](): void {} };
            const key = Symbol.dispose;
            r[Symbol.dispose]();
            using x = r;
          }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Pool.qml:5:25: error: `Symbol.dispose` is not in Ljós's API: it is only the key of a method in an object literal, as in `{ [Symbol.dispose](): void { … } }` [ts-forbidden]
        Pool.qml:6:15: error: `Symbol.dispose` is not in Ljós's API: it is only the key of a method in an object literal, as in `{ [Symbol.dispose](): void { … } }` [ts-forbidden]
        1 files, 2 errors, 0 warnings
        ljos: 2 errors
        """

    Scenario: A using declaration with no value is a syntax error
      Given the file "Pool.qml":
        """qml
        import Ljos.Ui
        Item {
          function work(): void {
            using r;
          }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Pool.qml:4:19: error: Using declarations must have an initializer. [syntax]
        Pool.qml:3:11: error: the script does not parse as JavaScript: Using declarations must have an initializer. [script-syntax]
        1 files, 2 errors, 0 warnings
        ljos: 2 errors
        """
