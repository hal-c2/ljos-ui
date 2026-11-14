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
  `undefined`, and that nothing assigns to it. Tier 1 and compiled code
  run a declaration whose value leaves nothing to dispose of, and hand
  any other to JavaScript.

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

  Rule: Tier 1 and compiled code run a using declaration that holds nothing to dispose of

    Scenario: The check counts a binding with a using declaration of undefined in tier 1 and compiled code
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item {
            property real count: 2
            property real half: { using held = undefined; return count / 2 }
            property real third: { using held = null; return count / 3 }
        }
        """
      When the files are checked with statistics
      Then the check reports the lines:
        """
        all        2 of 2 tier 1 (100.0%)
        compiled ahead of time  1 of 2 (50.0%)
        """
      And the check reports the lines:
        """
        Main.qml:5:34: runs dynamically: null
        """

    Scenario: Tier 1 runs a using declaration of undefined, and hands a disposable to JavaScript
      Given the tier 1 cross-check is on
      And the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property real count: 2
          property var held: undefined
          property string said: ""
          property real half: { using h = held; return count / 2 }
          function hold() { held = { [Symbol.dispose]() { root.said = root.said + "disposed"; } }; }
        }
        """
      When the property "count" of "root" is set to 4
      Then the property "half" of "root" is 2
      And the property "said" of "root" is ""
      When the method "hold" of "root" is called
      Then the property "said" of "root" is "disposed"
      And the log has no warning matching "tier 1 and tier 2 differ"

    Scenario: Compiled code runs a using declaration of undefined
      Given the files are compiled ahead of time
      And the compiled code cross-check is on
      And the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property real count: 2
          property real half: { using held = undefined; return count / 2 }
        }
        """
      When the property "count" of "root" is set to 6
      Then the property "half" of "root" is 3
      And the file "Scenario.qml" ran compiled code
      And the log has no warning matching "compiled code and the dynamic tiers differ"

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
          for (using each of [open("d"), open("e")]) { opened.push(each.name); }
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

    Scenario: What using, a for of with using or a stack's use is given needs a dispose method, and adopt a value
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
            for (using each of [1, 2]) {}
            stack.adopt();
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
        Pool.qml:9:32: error: `for (using …)` disposes of what it is given, and this is a number with no `[Symbol.dispose]()` method: give an object with one, or `undefined` [ts-assign-type]
        Pool.qml:10:13: error: this call needs 2 arguments, and is given 0 [ts-assign-type]
        1 files, 6 errors, 0 warnings
        ljos: 6 errors
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

    Scenario: Nothing assigns to a constant or to what a using declaration names
      Given the file "Pool.qml":
        """qml
        import Ljos.Ui
        Item {
          function work(): number {
            const size = 2;
            using held = undefined;
            let n = size;
            size = 3;
            held = undefined;
            size++;
            [n, size] = [n, 4];
            [n = (size = 3)] = [n];
            n = n + 1;
            return n;
          }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Pool.qml:7:13: error: `size` is a constant, and keeps the value it is declared with: declare it with `let` to assign it [ts-readonly]
        Pool.qml:8:13: error: `held` is declared with `using`, which disposes of the value it is given when its block ends: it is not assigned; declare another name [ts-readonly]
        Pool.qml:9:13: error: `size` is a constant, and keeps the value it is declared with: declare it with `let` to assign it [ts-readonly]
        Pool.qml:10:17: error: `size` is a constant, and keeps the value it is declared with: declare it with `let` to assign it [ts-readonly]
        Pool.qml:11:19: error: `size` is a constant, and keeps the value it is declared with: declare it with `let` to assign it [ts-readonly]
        1 files, 5 errors, 0 warnings
        ljos: 5 errors
        """

    Scenario: After a destructuring default, a name it assigns is what it was or what the default gave
      Given the file "Pool.qml":
        """qml
        import Ljos.Ui
        Item {
          function work(): string {
            let value: number | undefined = undefined;
            value = 1;
            let n = 1;
            [n = (value = 2)] = [value.toFixed().length];
            let other: number | undefined = undefined;
            [n = (other = 2)] = [n];
            let either: number | undefined = 1;
            either = 1;
            [n = (n > 0 ? (either = undefined) : (either = 2), 0)] = [n];
            return value.toFixed() + other.toFixed() + either.toFixed();
          }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Pool.qml:13:38: error: `other` may be undefined: compare it with `undefined` first [ts-assign-type]
        Pool.qml:13:56: error: `either` may be undefined: compare it with `undefined` first [ts-assign-type]
        1 files, 2 errors, 0 warnings
        ljos: 2 errors
        """

    Scenario: Nothing assigns to what a module imports
      Given the file "limits.lts":
        """ts
        export const limit: number = 2;
        """
      And the file "raise.lts":
        """ts
        import { limit } from "./limits.lts";
        export function raise(): number {
          limit = 3;
          return limit;
        }
        """
      And the file "tsconfig.json":
        """json
        { "extends": "./.ljos/tsconfig.base.json" }
        """
      When the files are checked
      Then the check reports:
        """
        raise.lts:3:11: error: `limit` is imported, and only the module that exports it changes it: declare a `let` with its value to change it here [ts-readonly]
        2 files, 1 errors, 0 warnings
        ljos: 1 errors
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
