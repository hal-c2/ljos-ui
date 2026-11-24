// The types of `ljos:test` (docs/adr/0005-ljos-test.md), for `tsc` and an
// editor. `ljos test` erases them; what runs is script/prelude.js.

declare module "ljos:test" {
  /** How long a retrying wait may let the application's clock run. */
  export interface Wait {
    /** Milliseconds of the application's time; 5000 unless the file's `test.use` says. */
    timeout?: number;
  }

  /** What a file's tests start, and how. */
  export interface Use extends Wait {
    /** The QML file, a path from the test file: `"./Main.qml"`. */
    app?: string;
    /** The window's size; the root's own when left out. */
    width?: number;
    height?: number;
  }

  export interface PictureOptions {
    /** How far a channel may be from the kept picture's (0 to 255); 2 unless given. */
    tolerance?: number;
    /** How many pixels may differ; 0 unless given. */
    maxDiffPixels?: number;
  }

  /** What an inspector tool answered. */
  export interface Answer {
    structured: unknown;
    text: string;
  }

  export interface ClickOptions {
    button?: "left" | "right" | "middle";
    count?: number;
  }

  /**
   * Text in the inspector's language (`#name`, `role=button[name="Save"]`,
   * `text=Hello`, `a >> b`). Each use finds the item again.
   */
  export interface Locator {
    find(inner: string): Locator;
    nth(index: number): Locator;
    click(options?: ClickOptions): Promise<void>;
    type(text: string): Promise<void>;
    press(keys: string, options?: { repeat?: number }): Promise<void>;
    scroll(by?: { dx?: number; dy?: number }): Promise<void>;
    property(name: string): Promise<unknown>;
    text(): Promise<string>;
    count(): Promise<number>;
    exists(): Promise<boolean>;
    inspect(): Promise<unknown>;
    toString(): string;
  }

  /** An application a test started, on a clock only the test moves. */
  export interface App {
    find(locator: string): Locator;
    click(target: string, options?: ClickOptions): Promise<void>;
    type(text: string, options?: { target?: string }): Promise<void>;
    press(keys: string, options?: { repeat?: number; target?: string }): Promise<void>;
    scroll(target: string, by?: { dx?: number; dy?: number }): Promise<void>;
    /** Lets `ms` of the application's time pass: its timers fire, its animations run. */
    advance(ms: number): Promise<void>;
    /** The application's clock, in milliseconds since it started. */
    now(): Promise<number>;
    resize(width: number, height: number): Promise<void>;
    /** The outline of the window an agent reads. */
    snapshot(): Promise<string>;
    logs(): Promise<unknown[]>;
    /** Retries `condition` on the application's clock until it gives something truthy. */
    waitFor<T>(condition: () => T | Promise<T>, options?: Wait): Promise<T>;
    /** Any other tool of the inspector. */
    call(tool: string, args?: Record<string, unknown>): Answer;
  }

  export interface ValueMatchers {
    toBe(expected: unknown): void;
    toEqual(expected: unknown): void;
    toBeTruthy(): void;
    toBeFalsy(): void;
    toBeNull(): void;
    toBeUndefined(): void;
    toBeDefined(): void;
    toContain(expected: unknown): void;
    toMatch(expected: string | RegExp): void;
    toHaveLength(expected: number): void;
    toBeGreaterThan(expected: number): void;
    toBeGreaterThanOrEqual(expected: number): void;
    toBeLessThan(expected: number): void;
    toBeLessThanOrEqual(expected: number): void;
    toBeCloseTo(expected: number, digits?: number): void;
    toThrow(expected?: string | RegExp): void;
  }

  export interface PictureMatchers {
    /** Compares with `__pictures__/<name>` beside the test file, which a run with none writes. */
    toMatchPicture(name: string, options?: PictureOptions): Promise<void>;
  }

  /** Each retries on the application's clock until it holds or its timeout passes. */
  export interface LocatorChecks {
    toHaveText(expected: string | RegExp, options?: Wait): Promise<void>;
    toHaveProperty(name: string, expected?: unknown, options?: Wait): Promise<void>;
    toHaveCount(expected: number, options?: Wait): Promise<void>;
    toExist(options?: Wait): Promise<void>;
    toBeVisible(options?: Wait): Promise<void>;
    toBeEnabled(options?: Wait): Promise<void>;
    toBeChecked(options?: Wait): Promise<void>;
    toBeFocused(options?: Wait): Promise<void>;
  }

  /** `toMatchPicture` has no `.not`: a picture is compared with one kept. */
  export interface LocatorMatchers extends LocatorChecks, PictureMatchers {}

  type Promised<M> = {
    [K in keyof M]: M[K] extends (...args: infer A) => unknown ? (...args: A) => Promise<void> : never;
  };

  export interface Expect {
    (actual: App): PictureMatchers;
    (actual: Locator): LocatorMatchers & { not: LocatorChecks };
    (actual: unknown): ValueMatchers & { not: ValueMatchers };
    /**
     * Retries `fn` on the clock of the application the test started last
     * until what it gives holds; a throw is retried too.
     */
    poll(fn: () => unknown, options?: Wait): Promised<ValueMatchers> & { not: Promised<ValueMatchers> };
  }

  /**
   * A test's function is given the application `test.use` names. A file
   * with no `test.use` starts its applications with `launch`, and leaves
   * the argument unused.
   */
  export interface Test {
    (name: string, fn: (app: App) => void | Promise<void>): void;
    skip(name: string, fn?: (app: App) => void | Promise<void>): void;
    only(name: string, fn: (app: App) => void | Promise<void>): void;
    /** What this file's tests start; once, at its top level. */
    use(options: Use): void;
  }

  export const test: Test;
  export const expect: Expect;
  /** Groups tests under a name; `fn` adds them at once, so it is not async. */
  export function describe(name: string, fn: () => undefined): void;
  /** Starts another application in a test: a second window, a second app. */
  export function launch(app: string, options?: Omit<Use, "app">): Promise<App>;
}
