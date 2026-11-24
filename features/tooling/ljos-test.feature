# Sources:
#   docs/adr/0005-ljos-test.md
#   docs/DIALECT.md section 4.4 (TypeScript modules)
#   docs/ARCHITECTURE.md section 19.4 (locators)
#   https://testanything.org/tap-version-14-specification.html
# Ljos.Test is Ljós's own test module: tests are TypeScript files that
# drive the application as an agent does, through locators, on the
# application's virtual clock. Qt has no such runner.

@ljos-only @headless
Feature: Tests in TypeScript that drive the application
  A `*.test.ts` file imports `test` and `expect` from "ljos:test", names
  the QML file it tests, and acts on it through the locators the
  inspector reads. `ljos test` runs every test with a fresh application
  and reports each one as a TAP line.

  Background:
    Given the file "Counter.qml":
      """qml
      import QtQuick
      import QtQuick.Controls
      Item {
        width: 200; height: 120
        property int count: 0
        Button { text: "Add"; onClicked: count++ }
        Button { text: "Locked"; y: 40; enabled: false; onClicked: count = 100 }
        Text { objectName: "count"; y: 80; text: count }
      }
      """

  Rule: Every test is run and reported

    Scenario: A test whose expectations hold is ok
      Given the file "counter.test.ts":
        """ts
        import { test, expect } from "ljos:test";
        test.use({ app: "./Counter.qml" });
        test("starts at nothing", async (app) => {
          expect(await app.find("#count").text()).toBe("0");
        });
        """
      When the tests are run
      Then the test report is:
        """
        TAP version 14
        ok 1 - counter.test.ts > starts at nothing
        1..1
        # pass 1
        # fail 0
        # skip 0
        """
      And the test run exits with status 0

    Scenario: A test whose expectation fails says what and where
      Given the file "counter.test.ts":
        """ts
        import { test, expect } from "ljos:test";
        test.use({ app: "./Counter.qml" });
        test("starts at one", async (app) => {
          expect(await app.find("#count").text()).toBe("1");
        });
        """
      When the tests are run
      Then the test report has the lines:
        """
        not ok 1 - counter.test.ts > starts at one
          ---
          message: 'expected "0" to be "1"'
          at: counter.test.ts:4
          ...
        """
      And the test run exits with status 1

    Scenario: A test that throws fails with what it threw
      Given the file "counter.test.ts":
        """ts
        import { test } from "ljos:test";
        test.use({ app: "./Counter.qml" });
        test("throws", async () => {
          throw new Error("not this way");
        });
        """
      When the tests are run
      Then the test report has the lines:
        """
        not ok 1 - counter.test.ts > throws
          ---
          message: 'Error: not this way'
          at: counter.test.ts:4
          ...
        """

    Scenario: A skipped test is reported and not run
      Given the file "counter.test.ts":
        """ts
        import { test, expect } from "ljos:test";
        test.use({ app: "./Counter.qml" });
        test.skip("not yet", async () => {
          expect(1).toBe(2);
        });
        test("runs", async () => {
          expect(1).toBe(1);
        });
        """
      When the tests are run
      Then the test report has the lines:
        """
        ok 1 - counter.test.ts > not yet # SKIP
        ok 2 - counter.test.ts > runs
        """
      And the test run exits with status 0

    Scenario: Tests in a describe block are named after it
      Given the file "counter.test.ts":
        """ts
        import { describe, test, expect } from "ljos:test";
        test.use({ app: "./Counter.qml" });
        describe("the counter", () => {
          test("is a number", async (app) => {
            expect(await app.find("#count").property("text")).toMatch(/^\d+$/);
          });
        });
        """
      When the tests are run
      Then the test report has the lines:
        """
        ok 1 - counter.test.ts > the counter > is a number
        """

    Scenario: Each test starts with an application of its own
      Given the file "counter.test.ts":
        """ts
        import { test, expect } from "ljos:test";
        test.use({ app: "./Counter.qml" });
        test("counts once", async (app) => {
          await app.click('role=button[name="Add"]');
          expect(await app.find("#count").text()).toBe("1");
        });
        test("counts once again", async (app) => {
          await app.click('role=button[name="Add"]');
          expect(await app.find("#count").text()).toBe("1");
        });
        """
      When the tests are run
      Then the test run exits with status 0

    Scenario: An application kept from an earlier test has ended
      Given the file "counter.test.ts":
        """ts
        import { test } from "ljos:test";
        import type { App } from "ljos:test";
        let kept: App | undefined;
        test.use({ app: "./Counter.qml" });
        test("keeps its application", async (app) => {
          kept = app;
        });
        test("uses the one it kept", async () => {
          await kept!.click('role=button[name="Add"]');
        });
        """
      When the tests are run
      Then the test report has the lines:
        """
        ok 1 - counter.test.ts > keeps its application
        not ok 2 - counter.test.ts > uses the one it kept
          ---
          message: 'the application has ended'
          at: counter.test.ts:9
          ...
        """

    Scenario: Qt Quick Test files and TypeScript tests run together
      Given the file "tst_sums.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          name: "Sums"
          function test_sum() { compare(1 + 1, 3) }
        }
        """
      And the file "counter.test.ts":
        """ts
        import { test, expect } from "ljos:test";
        test.use({ app: "./Counter.qml" });
        test("starts at one", async (app) => {
          expect(await app.find("#count").text()).toBe("1");
        });
        """
      When the tests are run
      Then the test report has the lines:
        """
        FAIL!  : Sums::test_sum() Compared values are not the same
        """
      And the test report has the lines:
        """
        not ok 1 - counter.test.ts > starts at one
        """
      And the test results' "/totals/failed" is 2
      And the test results' "/tests/1/function" is "test_sum"
      And the test results' "/tests/1/outcome" is "fail"
      And the test results' "/tests/3/file" is "counter.test.ts"
      And the test results' "/tests/3/outcome" is "fail"
      And the test run exits with status 2

  Rule: A test acts on what a user could reach

    Scenario: A click on a button found by its role and name
      Given the file "counter.test.ts":
        """ts
        import { test, expect } from "ljos:test";
        test.use({ app: "./Counter.qml" });
        test("a click counts", async (app) => {
          const add = app.find('role=button[name="Add"]');
          await add.click();
          await add.click();
          await expect(app.find("#count")).toHaveText("2");
        });
        """
      When the tests are run
      Then the test run exits with status 0

    Scenario: A disabled button is not clicked, and the test is told why
      Given the file "counter.test.ts":
        """ts
        import { test } from "ljos:test";
        test.use({ app: "./Counter.qml" });
        test("the lock holds", async (app) => {
          await app.click('role=button[name="Locked"]');
        });
        """
      When the tests are run
      Then the test report has the lines:
        """
        not ok 1 - counter.test.ts > the lock holds
        """
      And the test results' "/tests/0/message" contains "is disabled"
      And the test results' "/tests/0/at" is "counter.test.ts:4"

    Scenario: Typed text reaches the focused field
      Given the file "Form.qml":
        """qml
        import QtQuick
        import QtQuick.Controls
        Item {
          width: 200; height: 80
          TextField { objectName: "who"; width: 180; placeholderText: "Name" }
        }
        """
      And the file "form.test.ts":
        """ts
        import { test, expect } from "ljos:test";
        test.use({ app: "./Form.qml" });
        test("a name is typed", async (app) => {
          await app.find("#who").type("Ada");
          await expect(app.find("#who")).toHaveProperty("text", "Ada");
        });
        """
      When the tests are run
      Then the test run exits with status 0

    Scenario: Counting what a locator finds
      Given the file "counter.test.ts":
        """ts
        import { test, expect } from "ljos:test";
        test.use({ app: "./Counter.qml" });
        test("two buttons", async (app) => {
          expect(await app.find("role=button").count()).toBe(2);
          await expect(app.find('role=button[name="Gone"]')).toHaveCount(0);
        });
        """
      When the tests are run
      Then the test run exits with status 0

  Rule: Time moves only when a test lets it

    Background:
      Given the file "Later.qml":
        """qml
        import QtQuick
        Item {
          width: 100; height: 40
          Text { objectName: "said"; text: "wait" }
          Timer {
            interval: 1000; running: true
            onTriggered: parent.children[0].text = "done"
          }
        }
        """

    Scenario: A retried expectation lets the application's time pass
      Given the file "later.test.ts":
        """ts
        import { test, expect } from "ljos:test";
        test.use({ app: "./Later.qml" });
        test("it is done in a second", async (app) => {
          expect(await app.find("#said").text()).toBe("wait");
          await expect(app.find("#said")).toHaveText("done");
        });
        """
      When the tests are run
      Then the test run exits with status 0

    Scenario: A retried expectation gives up at its timeout
      Given the file "later.test.ts":
        """ts
        import { test, expect } from "ljos:test";
        test.use({ app: "./Later.qml" });
        test("too soon", async (app) => {
          await expect(app.find("#said")).toHaveText("done", { timeout: 500 });
        });
        """
      When the tests are run
      Then the test report has the lines:
        """
        not ok 1 - later.test.ts > too soon
          ---
          message: 'expected #said to have text "done"; it was "wait" after 500 ms'
          at: later.test.ts:4
          ...
        """

    Scenario: Advancing the clock fires what is due
      Given the file "later.test.ts":
        """ts
        import { test, expect } from "ljos:test";
        test.use({ app: "./Later.qml" });
        test("999 then 1", async (app) => {
          await app.advance(999);
          expect(await app.find("#said").text()).toBe("wait");
          await app.advance(1);
          expect(await app.find("#said").text()).toBe("done");
        });
        """
      When the tests are run
      Then the test run exits with status 0

    Scenario: waitFor retries any condition on the application's clock
      Given the file "later.test.ts":
        """ts
        import { test, expect } from "ljos:test";
        test.use({ app: "./Later.qml" });
        test("waits", async (app) => {
          await app.waitFor(async () => (await app.find("#said").text()) === "done");
          expect(await app.now()).toBeGreaterThan(999);
        });
        """
      When the tests are run
      Then the test run exits with status 0

  Rule: A picture is compared with the one kept beside the test

    Scenario: A picture with none kept is written, and then held to
      Given the file "counter.test.ts":
        """ts
        import { test, expect } from "ljos:test";
        test.use({ app: "./Counter.qml" });
        test("first look", async (app) => {
          await expect(app).toMatchPicture("counter.png");
        });
        test("same look", async (app) => {
          await expect(app).toMatchPicture("counter.png");
        });
        test("after a click", async (app) => {
          await app.click('role=button[name="Add"]');
          await expect(app).toMatchPicture("counter.png");
        });
        """
      When the tests are run
      Then the test report has the lines:
        """
        not ok 1 - counter.test.ts > first look
          ---
          message: 'there was no __pictures__/counter.png; this run''s picture is written there: look at it, and run again'
          at: counter.test.ts:4
          ...
        ok 2 - counter.test.ts > same look
        not ok 3 - counter.test.ts > after a click
        """
      And the test results' "/tests/2/message" contains "the picture differs from __pictures__/counter.png in"
      And the test results' "/tests/2/message" contains "What this run saw is in __pictures__/counter.actual.png"
      And the test results' "/tests/2/at" is "counter.test.ts:11"

    Scenario: A picture of one item is of that item alone
      Given the file "counter.test.ts":
        """ts
        import { test, expect } from "ljos:test";
        test.use({ app: "./Counter.qml" });
        test("the count", async (app) => {
          await expect(app.find("#count")).toMatchPicture("count.png");
        });
        test("the count in a larger window", async (app) => {
          await app.resize(300, 200);
          await expect(app.find("#count")).toMatchPicture("count.png");
        });
        """
      When the tests are run
      Then the test report has the lines:
        """
        not ok 1 - counter.test.ts > the count
        ok 2 - counter.test.ts > the count in a larger window
        """

    Scenario: A run that keeps pictures passes and keeps them
      Given the file "counter.test.ts":
        """ts
        import { test, expect } from "ljos:test";
        test.use({ app: "./Counter.qml" });
        test("before", async (app) => {
          await expect(app).toMatchPicture("counter.png");
        });
        test("after a click", async (app) => {
          await app.click('role=button[name="Add"]');
          await expect(app).toMatchPicture("counter.png");
        });
        """
      When the tests are run keeping their pictures
      Then the test run exits with status 0

    Scenario: A picture is named by a file name
      Given the file "counter.test.ts":
        """ts
        import { test, expect } from "ljos:test";
        test.use({ app: "./Counter.qml" });
        test("elsewhere", async (app) => {
          await expect(app).toMatchPicture("../counter.png");
        });
        """
      When the tests are run
      Then the test results' "/tests/0/message" contains "with no directory"
      And the test run exits with status 1

  Rule: A test file is TypeScript

    Scenario: Types are erased and helpers beside the test are imported
      Given the file "steps.ts":
        """ts
        import type { App } from "ljos:test";
        export async function addTimes(app: App, times: number): Promise<void> {
          for (let i = 0; i < times; i++) await app.click('role=button[name="Add"]');
        }
        """
      And the file "counter.test.ts":
        """ts
        import { test, expect } from "ljos:test";
        import { addTimes } from "./steps.ts";
        test.use({ app: "./Counter.qml" });
        test("three", async (app) => {
          await addTimes(app, 3);
          const shown: string = await app.find("#count").text();
          expect(shown).toBe("3");
        });
        """
      When the tests are run
      Then the test run exits with status 0

    Scenario: A module other than ljos:test or a file beside it is refused
      Given the file "counter.test.ts":
        """ts
        import { test } from "vitest";
        test("never", () => {});
        """
      When the tests are run
      Then the test report has the lines:
        """
        not ok 1 - counter.test.ts
        """
      And the test results' "/tests/0/message" contains "`vitest` is not a file beside the module"
      And the test results' "/tests/0/at" contains "counter.test.ts:1:"
      And the test run exits with status 1
