// A module the decoder cases beside it import: a guard and an assertion function.
export interface Row { tag: string; count: number }
export function isRow(v: unknown): v is Row {
    return typeof v === "object" && v !== null && "tag" in v && typeof v.tag === "string" && "count" in v && typeof v.count === "number"
}
export function isTag(v: unknown): v is string { return typeof v === "string" }
export function row(v: unknown): asserts v is Row {
    if (!isRow(v)) { throw new Error("not a row") }
}
