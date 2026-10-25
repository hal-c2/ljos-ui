# ADR 0002: Component plans, linked by the runtime on first use

- **Status:** accepted, 2026-10-25. Built in batch B24 of the delegate
  track.
- **Terms:** [CONTEXT.md](../../CONTEXT.md) (Component plan, Row plan).

## Context

Every instance of a QML component today repeats work that depends only on
the component and the types it uses, not on the instance:

- a member of a native object (`anchors.verticalCenter`, `width`) is found
  by name (`member_target`, `MetaObject::property_index`);
- installing a binding clones the declaring meta, builds the binding's
  source location and interns its URL;
- each native object looks its type up in the registry by module and name;
- each instance fills a hash of its ids in its context;
- each evaluation reads its free names by name (`read_by_name`) and finds
  attached objects through the registry, three lookups a run.

`crates/ljos_quick/tests/budget.rs` counts 19.1 member and 2 type lookups by
name per ListView delegate, and 6.1 and 1 per plain Item with a Text. All
of them give the same answer for every instance of the component.

The obvious fix is to have the compiler resolve them and write the answers
into the IR. That ties the IR to the native types the program was compiled
against: add a property to `Item`, and every compiled file that names a
property after it holds a wrong index until it is compiled again. The IR is
also what a shipped bundle and a hot reload carry (section 12), so a stale
index there is a wrong write at run time, not a compile error.

## Decision

The work is split by what it depends on.

- **The compiler emits only facts that do not depend on native types:**
  each id's object slot in its component, the indexes of the properties a
  QML object declares itself, the shape of each expression's parameters
  (which free names it reads and what each one is: an id, a property of
  its own object, of the context, an attached type) and per-object flags
  (has bindings, has handlers, needs a context of its own). The IR format
  version goes up when they are added.
- **The runtime links the rest on first use** and keeps it beside
  `LoadedFile` (`ljos_engine/src/loader.rs`): the native property indexes
  each binding writes and each expression reads, the metas of the objects
  a component makes, the attached types it uses. This linked form is the
  *component plan*. It is built when the first instance is made and
  reused by every later one; a reload that replaces the file or a module
  registered later drops it.
- **Instancing a component is then** allocating its objects, indexed
  writes and indexed binding installs. A binding's source location is
  derived from the plan when a diagnostic needs it, not built per install.
  Ids resolve by slot, not through a per-instance hash. An evaluation's
  free names are slots in the plan, so a run makes no lookup by name.

## Consequences

- A change to a native type never leaves IR stale: the IR names native
  members by name, and the runtime resolves the names against the types it
  actually has, once per component.
- The budget tests pin lookups by name per instance; the plan's goal is
  zero for both cases once B24 lands. The first instance still pays the
  linking, which the tests keep out of the count by warming the component
  first.
- A plan is per loaded file and per engine, as `LoadedFile` is, so two
  engines never share one and a plan never outlives the types it was
  linked against.
- `Qt.createQmlObject` and files compiled at run time get a plan the same
  way: the runtime links whatever IR it is given.
- The row plan (B25) is the same idea for what a view hands each delegate:
  per component and model roles, rebuilt when the model's roles change.
