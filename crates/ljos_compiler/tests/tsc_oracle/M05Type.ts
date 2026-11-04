// A module the case beside it imports.
export interface Row { tag: string; count: number }
export type Tag = "a" | "b"
export function only(tag: Tag): number { return tag === "a" ? 2 : 3 }
export const label: string = "rows"
