# Types

Milestone M4 in [DIALECT.md §13](../../DIALECT.md#13-milestones). Written only by the lane building this area.

Built:

- TS modules
- `property T` of their types
- `tsc` given the modules (§4.4)
- decoders (type guards and assertion functions narrowing `unknown` as TypeScript 5 does)
- `JSON.parse` and `property unknown` as `unknown`
- `ts-boundary`, a `.js` typed by the `.d.ts` beside it (§4.4, §5.1)
- `state` as the union of the names a file's states are given (§4.9)

Remaining:

- tier-1 typed opcodes
- `XmlModel`'s generated decoder (M1, M5)
- the other untyped JS globals (`XMLHttpRequest`'s `response`, outside the subset today)
