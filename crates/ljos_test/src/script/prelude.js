// The `ljos:test` module (docs/adr/0005-ljos-test.md), run in a test
// file's own QuickJS context. `host(op, json)` is the runner: it starts
// applications and answers the inspector's tools, synchronously, and gives
// back `{"ok": …}` or `{"error": "…"}` as JSON.
//
// An act does its work before it returns, so an error it throws has the
// test's own line on its stack; it returns a promise only so that a test
// reads as any TypeScript test does (`await app.click(…)`). What awaits a
// function of the test's (`waitFor`, `expect.poll`) notes the test's line
// before its first `await`.
(function (host, testFile) {
  "use strict";

  const STEP_MS = 16;
  const DEFAULT_TIMEOUT_MS = 5000;

  function ask(op, arg) {
    const answer = JSON.parse(host(op, JSON.stringify(arg === undefined ? null : arg)));
    if (Object.prototype.hasOwnProperty.call(answer, "error")) {
      throw new TestError(answer.error);
    }
    return answer.ok;
  }

  // The line of the test file a stack was taken on: the first frame that
  // is not this module's.
  function siteOf(stack) {
    for (const line of String(stack || "").split("\n")) {
      const m = /\(?([^\s()]+):(\d+)(?::(\d+))?\)?\s*$/.exec(line.trim());
      if (!m || m[1] === "ljos:test" || m[1] === "<native>") continue;
      return m[1] + ":" + m[2];
    }
    return null;
  }

  class TestError extends Error {
    constructor(message, site) {
      super(message);
      this.name = "TestError";
      if (site) this.at = site;
    }
  }

  function show(value) {
    if (typeof value === "string") return JSON.stringify(value);
    if (typeof value === "function") return "a function";
    if (value instanceof RegExp) return String(value);
    if (value instanceof Locator) return value.toString();
    try {
      const text = JSON.stringify(value);
      return text === undefined ? String(value) : text;
    } catch (_) {
      return String(value);
    }
  }

  function equal(a, b) {
    if (Object.is(a, b)) return true;
    if (typeof a !== "object" || typeof b !== "object" || a === null || b === null) return false;
    if (Array.isArray(a) !== Array.isArray(b)) return false;
    const ka = Object.keys(a);
    const kb = Object.keys(b);
    if (ka.length !== kb.length) return false;
    return ka.every((k) => Object.prototype.hasOwnProperty.call(b, k) && equal(a[k], b[k]));
  }

  // ------------------------------------------------------------ tests

  const tests = [];
  const path = [];
  let config = {};
  let focused = false;

  function register(name, fn, mode) {
    if (typeof name !== "string") throw new TestError("a test is named by a string");
    if (typeof fn !== "function") throw new TestError(`the test ${show(name)} has no function`);
    if (mode === "only") focused = true;
    tests.push({ name: path.concat([name]), fn, mode });
  }

  function test(name, fn) {
    register(name, fn, "run");
  }
  test.skip = (name, fn) => register(name, fn || (() => {}), "skip");
  test.only = (name, fn) => register(name, fn, "only");
  test.use = (options) => {
    config = Object.assign({}, config, options);
  };

  function describe(name, fn) {
    path.push(name);
    try {
      fn();
    } finally {
      path.pop();
    }
  }

  // ------------------------------------------------------------ the app

  class App {
    #id;
    constructor(id) {
      this.#id = id;
    }
    get id() {
      return this.#id;
    }
    call(tool, args) {
      return ask("call", { app: this.#id, tool, args: args || {} });
    }
    find(locator) {
      return new Locator(this, String(locator));
    }
    click(target, options) {
      return this.find(target).click(options);
    }
    type(text, options) {
      const args = { text: String(text) };
      if (options && options.target) args.target = String(options.target);
      this.call("app_type", args);
      return Promise.resolve();
    }
    press(keys, options) {
      const args = { keys: String(keys) };
      if (options && options.repeat) args.repeat = options.repeat;
      if (options && options.target) args.target = String(options.target);
      this.call("app_press", args);
      return Promise.resolve();
    }
    scroll(target, by) {
      return this.find(target).scroll(by);
    }
    advance(ms) {
      this.step(ms);
      return Promise.resolve();
    }
    step(ms) {
      if (!(ms >= 0)) throw new TestError(`advance takes milliseconds, not ${show(ms)}`);
      if (ms > 0) this.call("app_advance", { ms: Math.round(ms) });
    }
    now() {
      return Promise.resolve(ask("now", { app: this.#id }));
    }
    resize(width, height) {
      this.call("app_resize", { width, height });
      return Promise.resolve();
    }
    snapshot() {
      return Promise.resolve(this.call("app_snapshot", {}).text);
    }
    logs() {
      const answer = this.call("app_logs", {});
      return Promise.resolve(answer.structured && answer.structured.entries ? answer.structured.entries : []);
    }
    // Retries `condition` on the app's clock until it gives something
    // truthy, which it then gives.
    async waitFor(condition, options) {
      const site = siteOf(new Error().stack);
      const timeout = timeoutOf(options);
      let waited = 0;
      let last;
      for (;;) {
        let value;
        try {
          value = await condition();
          last = undefined;
        } catch (e) {
          value = false;
          last = e;
        }
        if (value) return value;
        if (waited >= timeout) {
          const why = last ? `; it last threw ${messageOf(last)}` : "";
          throw new TestError(`the condition did not hold after ${timeout} ms${why}`, site);
        }
        const step = Math.min(STEP_MS, timeout - waited);
        this.step(step);
        waited += step;
      }
    }
    toString() {
      return "the application";
    }
  }

  function timeoutOf(options) {
    const t = options && options.timeout;
    if (t === undefined) return config.timeout === undefined ? DEFAULT_TIMEOUT_MS : config.timeout;
    if (!(t >= 0)) throw new TestError(`a timeout is milliseconds, not ${show(t)}`);
    return t;
  }

  function messageOf(e) {
    if (e instanceof Error) return e.name === "TestError" ? e.message : `${e.name}: ${e.message}`;
    return show(e);
  }

  // A locator is text in the inspector's language; each use finds the item
  // again.
  class Locator {
    constructor(app, text) {
      this.app = app;
      this.text = text;
    }
    toString() {
      return this.text;
    }
    find(inner) {
      return new Locator(this.app, `${this.text} >> ${inner}`);
    }
    nth(index) {
      return new Locator(this.app, `${this.text} >> nth=${index}`);
    }
    click(options) {
      const args = { target: this.text };
      if (options && options.button) args.button = options.button;
      if (options && options.count) args.count = options.count;
      this.app.call("app_click", args);
      return Promise.resolve();
    }
    type(text) {
      this.app.call("app_type", { text: String(text), target: this.text });
      return Promise.resolve();
    }
    press(keys, options) {
      const args = { keys: String(keys), target: this.text };
      if (options && options.repeat) args.repeat = options.repeat;
      this.app.call("app_press", args);
      return Promise.resolve();
    }
    scroll(by) {
      const args = { target: this.text };
      if (by && by.dx !== undefined) args.dx = by.dx;
      if (by && by.dy !== undefined) args.dy = by.dy;
      this.app.call("app_scroll", args);
      return Promise.resolve();
    }
    matches() {
      const answer = this.app.call("app_find", { locator: this.text, limit: 1 });
      return answer.structured;
    }
    countNow() {
      return this.matches().total;
    }
    count() {
      return Promise.resolve(this.countNow());
    }
    exists() {
      return Promise.resolve(this.countNow() > 0);
    }
    inspectNow() {
      return this.app.call("app_inspect", { target: this.text, all: true }).structured;
    }
    inspect() {
      return Promise.resolve(this.inspectNow());
    }
    // What the inspector shows of the one item found, or null when there
    // is none.
    seenNow() {
      return this.countNow() === 0 ? null : this.inspectNow();
    }
    propertyOf(seen, name) {
      const row = (seen.properties || []).find((p) => p.name === name);
      if (!row) throw new TestError(`${this.text} has no property ${show(name)}`);
      return valueOf(row);
    }
    propertyNow(name) {
      return this.propertyOf(this.inspectNow(), name);
    }
    property(name) {
      return Promise.resolve(this.propertyNow(name));
    }
    text() {
      return Promise.resolve(String(this.propertyNow("text")));
    }
  }

  // A property as the inspector shows it, read back as the value a script
  // would see: numbers and truths by their type, everything else as text.
  function valueOf(row) {
    const v = row.value;
    if (v === null || v === undefined) return v;
    if (typeof row.number === "number" && typeof v === "string") return v;
    switch (row.type) {
      case "int":
      case "real":
      case "double":
      case "float":
      case "qreal": {
        const n = Number(v);
        return Number.isNaN(n) && v !== "NaN" ? v : n;
      }
      case "bool":
        return v === true || v === "true";
      default:
        return v;
    }
  }

  // ------------------------------------------------------------ expect

  // What a value matcher decides: whether it holds, and how to say it does
  // not (with `not` when it was asked not to).
  const valueMatchers = {
    toBe: (a, b) => [Object.is(a, b), "to be", b],
    toEqual: (a, b) => [equal(a, b), "to equal", b],
    toBeTruthy: (a) => [!!a, "to be truthy"],
    toBeFalsy: (a) => [!a, "to be falsy"],
    toBeNull: (a) => [a === null, "to be null"],
    toBeUndefined: (a) => [a === undefined, "to be undefined"],
    toBeDefined: (a) => [a !== undefined, "to be defined"],
    toContain: (a, b) => [a != null && typeof a.includes === "function" && a.includes(b), "to contain", b],
    toMatch: (a, b) => [
      typeof a === "string" && (b instanceof RegExp ? b.test(a) : a.includes(String(b))),
      "to match",
      b,
    ],
    toHaveLength: (a, b) => [a != null && a.length === b, "to have length", b],
    toBeGreaterThan: (a, b) => [a > b, "to be greater than", b],
    toBeGreaterThanOrEqual: (a, b) => [a >= b, "to be at least", b],
    toBeLessThan: (a, b) => [a < b, "to be less than", b],
    toBeLessThanOrEqual: (a, b) => [a <= b, "to be at most", b],
    toBeCloseTo: (a, b, digits) => [
      Math.abs(a - b) < Math.pow(10, -(digits === undefined ? 2 : digits)) / 2,
      "to be close to",
      b,
    ],
  };

  function says(actual, negated, verdict) {
    const [, words, ...rest] = verdict;
    const expected = rest.length > 0 ? " " + show(rest[0]) : "";
    return `expected ${show(actual)} ${negated ? "not " : ""}${words}${expected}`;
  }

  function valueExpectation(actual, negated) {
    const out = {};
    for (const [name, matcher] of Object.entries(valueMatchers)) {
      out[name] = (...args) => {
        const verdict = matcher(actual, ...args);
        if (verdict[0] === negated) throw new TestError(says(actual, negated, verdict));
      };
    }
    out.toThrow = (expected) => {
      if (typeof actual !== "function") throw new TestError("toThrow is given a function to call");
      let thrown = null;
      let threw = false;
      try {
        actual();
      } catch (e) {
        threw = true;
        thrown = e;
      }
      let holds = threw;
      if (threw && expected !== undefined) {
        const m = thrown instanceof Error ? thrown.message : String(thrown);
        holds = expected instanceof RegExp ? expected.test(m) : m.includes(String(expected));
      }
      if (holds === negated) {
        const what = expected === undefined ? "" : " " + show(expected);
        throw new TestError(
          negated
            ? `expected the function not to throw${what}; it threw ${messageOf(thrown)}`
            : threw
              ? `expected the function to throw${what}; it threw ${messageOf(thrown)}`
              : `expected the function to throw${what}; it returned`,
        );
      }
    };
    return out;
  }

  // How a locator matcher says what it expects, what it reads each time it
  // looks, and how it says what it saw. An item that is not there has no
  // text, is not shown and not enabled: under `not` that holds. Anything
  // else that stops a look (a locator that finds more than one item, a
  // tool the inspector refuses) never holds, with or without `not`.
  const NOT_THERE = "not there";
  const locatorMatchers = {
    toHaveText: [1, (want) => `to have text ${show(want)}`, (loc, want) => {
      const seen = loc.seenNow();
      if (!seen) return [false, NOT_THERE];
      const text = String(loc.propertyOf(seen, "text"));
      return [want instanceof RegExp ? want.test(text) : text === String(want), show(text)];
    }],
    // With no value, that it has the property at all.
    toHaveProperty: [
      2,
      (name, ...want) => (want.length === 0 ? `to have ${name}` : `to have ${name} ${show(want[0])}`),
      (loc, name, ...want) => {
        const seen = loc.seenNow();
        if (!seen) return [false, NOT_THERE];
        const value = loc.propertyOf(seen, name);
        return [want.length === 0 || equal(value, want[0]), show(value)];
      },
    ],
    toHaveCount: [1, (want) => `to match ${want} item${want === 1 ? "" : "s"}`, (loc, want) => {
      const n = loc.countNow();
      return [n === want, `${n}`];
    }],
    toExist: [0, () => "to match an item", (loc) => {
      const n = loc.countNow();
      return [n > 0, `${n}`];
    }],
    toBeVisible: [0, () => "to be shown", (loc) => {
      const seen = loc.seenNow();
      if (!seen) return [false, NOT_THERE];
      const s = seen.state || {};
      const shown = s.visible === true && s.lattice !== "hidden";
      return [shown, shown ? "shown" : "hidden"];
    }],
    toBeEnabled: [0, () => "to be enabled", (loc) => {
      const seen = loc.seenNow();
      if (!seen) return [false, NOT_THERE];
      const s = seen.state || {};
      return [s.enabled === true, s.enabled ? "enabled" : "disabled"];
    }],
    toBeChecked: [0, () => "to be checked", (loc) => {
      const seen = loc.seenNow();
      if (!seen) return [false, NOT_THERE];
      const checked = loc.propertyOf(seen, "checked");
      return [checked === true, checked ? "checked" : "not checked"];
    }],
    toBeFocused: [0, () => "to have the focus", (loc) => {
      const seen = loc.seenNow();
      if (!seen) return [false, NOT_THERE];
      const s = seen.state || {};
      return [s.focus === true, s.focus ? "focused" : "not focused"];
    }],
  };

  // Retries a matcher on the app's clock: once at once, then a frame at a
  // time, up to the timeout. A look that fails is retried too, and at the
  // timeout says why it failed.
  function locatorExpectation(loc, negated) {
    const out = {};
    for (const [name, [arity, words, look]] of Object.entries(locatorMatchers)) {
      out[name] = (...args) => {
        const last = args[args.length - 1];
        const options =
          args.length > arity && last && typeof last === "object" && !(last instanceof RegExp)
            ? args.pop()
            : undefined;
        const timeout = timeoutOf(options);
        let waited = 0;
        for (;;) {
          let verdict = null;
          let failed = null;
          try {
            verdict = look(loc, ...args);
          } catch (e) {
            failed = messageOf(e);
          }
          if (verdict && verdict[0] !== negated) return Promise.resolve();
          if (waited >= timeout) {
            const expected = `expected ${loc.text} ${negated ? "not " : ""}${words(...args)}`;
            const after = timeout > 0 ? ` after ${timeout} ms` : "";
            throw new TestError(
              verdict
                ? `${expected}; it was ${verdict[1]}${after}`
                : `${expected}; it could not be looked at${after}: ${failed}`,
            );
          }
          const step = Math.min(STEP_MS, timeout - waited);
          loc.app.step(step);
          waited += step;
        }
      };
    }
    return out;
  }

  // `expect(app).toMatchPicture("name.png")`, or of one item with a
  // locator: the picture is compared with the one kept beside the test,
  // once, after the act before it settled the scene.
  function pictureMatcher(app, target, negated) {
    return (name, options) => {
      if (negated) throw new TestError("toMatchPicture has no `not`: a picture is compared with one kept");
      const o = options || {};
      ask("picture", {
        app: app.id,
        from: testFile,
        target,
        name: String(name),
        tolerance: o.tolerance,
        maxDiffPixels: o.maxDiffPixels,
      });
      return Promise.resolve();
    };
  }

  function expect(actual) {
    const make = (negated) => {
      if (actual instanceof App) return { toMatchPicture: pictureMatcher(actual, null, negated) };
      if (!(actual instanceof Locator)) return valueExpectation(actual, negated);
      const out = locatorExpectation(actual, negated);
      out.toMatchPicture = pictureMatcher(actual.app, actual.text, negated);
      return out;
    };
    const out = make(false);
    out.not = make(true);
    return out;
  }

  // `expect.poll(fn).toBe(x)`: retries `fn` on the app's clock until what
  // it gives holds.
  expect.poll = (fn, options) => {
    const make = (negated) => {
      const out = {};
      for (const name of Object.keys(valueMatchers)) {
        out[name] = async (...args) => {
          const site = siteOf(new Error().stack);
          const app = latest;
          if (!app) throw new TestError("expect.poll needs an application: give one with test.use or launch", site);
          const timeout = timeoutOf(options);
          let waited = 0;
          for (;;) {
            let actual;
            let verdict = null;
            let thrown = null;
            try {
              actual = await fn();
              verdict = valueMatchers[name](actual, ...args);
            } catch (e) {
              thrown = e;
            }
            if (verdict && verdict[0] !== negated) return;
            if (waited >= timeout) {
              throw new TestError(
                verdict
                  ? `${says(actual, negated, verdict)} after ${timeout} ms`
                  : `the poll did not give what was expected after ${timeout} ms; it last threw ${messageOf(thrown)}`,
                site,
              );
            }
            const step = Math.min(STEP_MS, timeout - waited);
            app.step(step);
            waited += step;
          }
        };
      }
      return out;
    };
    const out = make(false);
    out.not = make(true);
    return out;
  };

  function launch(app, options) {
    const o = Object.assign({}, config, options || {});
    const id = ask("launch", { from: testFile, app: String(app), width: o.width, height: o.height });
    latest = new App(id);
    return Promise.resolve(latest);
  }

  // ------------------------------------------------------------ running

  let current = null;
  // The application the test started last: the one `expect.poll` lets
  // time pass in.
  let latest = null;
  const states = [];

  function list() {
    return JSON.stringify(
      tests.map((t) => ({
        name: t.name,
        skip: t.mode === "skip" || (focused && t.mode !== "only"),
      })),
    );
  }

  function start(index) {
    const t = tests[index];
    const state = { done: false };
    states[index] = state;
    current = null;
    latest = null;
    let promise;
    try {
      if (config.app !== undefined) {
        const id = ask("launch", {
          from: testFile,
          app: String(config.app),
          width: config.width,
          height: config.height,
        });
        current = new App(id);
        latest = current;
      }
      promise = Promise.resolve(t.fn(current));
    } catch (e) {
      promise = Promise.reject(e);
    }
    promise.then(
      () => {
        state.done = true;
      },
      (e) => {
        state.done = true;
        state.error = {
          message: messageOf(e),
          at: (e && e.at) || siteOf(e && e.stack),
        };
      },
    );
  }

  function result(index) {
    return JSON.stringify(states[index] || { done: false });
  }

  const api = Object.freeze({ test, describe, expect, launch });
  return { api, list, start, result };
});
