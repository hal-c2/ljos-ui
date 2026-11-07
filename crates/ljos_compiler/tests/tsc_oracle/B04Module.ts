// A module that reaches a JavaScript file through its declarations.
import { add } from "./B01Declared.js"
export function twice(n: number): number { return add(n, n) }
