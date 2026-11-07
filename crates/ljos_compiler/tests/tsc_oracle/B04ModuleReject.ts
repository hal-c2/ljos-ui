// A module that calls a JavaScript file as its declarations do not say.
import { add } from "./B01Declared.js"
export function twice(n: number): string { return add(n, n) }
