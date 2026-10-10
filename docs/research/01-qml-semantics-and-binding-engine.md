# 01: QML language semantics and the runtime object and binding model

*Ljós UI research note. Written 2026-10-10. The sources are doc.qt.io (which currently documents Qt 6.12), Qt blog and mailing-list posts, docs.rs, crates.io and a measured survey of the HAL-C2 QML code. No Qt or Slint source code was read; search results that pointed into Qt source (codebrowser, code.qt.io, contribute.qt-project.org doxygen) were deliberately skipped.*

---

## 1. Summary of key findings

- **QML is a dynamically scoped, eagerly notified object language with JS expressions as values.** The semantics Ljós must reproduce are the following:
  - **Name lookup order:** JS local scope, then `id`s in the component, then properties of the scope object, then properties of the component root, then enclosing component contexts, then the JS global object.
  - **Imperative assignment from JS breaks a binding.** `Qt.binding()` replaces it instead.
  - **Change handlers fire synchronously.** They run from within the setter.
  - **Some orders are explicitly undefined.** The docs say binding evaluation order and `Component.onCompleted` order are not guaranteed.
- **Qt 6 tried lazy bindings and backed out.** Qt 6.0 `QProperty` evaluated bindings when they were read. Qt 6.2 switched to eager evaluation plus *property update groups* (`Qt::beginPropertyUpdateGroup`, `QScopedPropertyUpdateGroup`). Qt gave two reasons: about 50% of existing properties needed immediate evaluation, and lazy evaluation would have run bindings during the render-thread sync. Ljós does not have Qt's threaded scene-graph sync, so it can safely be *lazy for render-only consumers and eager for observed ones*.
- **Qt's binding model is not glitch-free, and apps rely on its quirks.** Two patterns recur:
  - A handler fires once per intermediate value, for example twice for `c: a+b` after `a=…; b=…`.
  - A "guard flag" around an assignment suppresses a handler, which only works because handlers are synchronous.

  So Ljós must keep handlers synchronous, while ordering pure recomputation glitch-free within each flush.
- **AOT compilation in Qt works only on code it can fully analyse.** qmlcachegen produces bytecode for everything and native code only for "functions and bindings that compiler fully understands". qmlsc (commercial) adds `--static` and `--direct-calls`. Both depend on type knowledge. **HAL-C2 has almost no type annotations:**
  - 4 of 539 functions have typed signatures.
  - 573 of 1830 properties (31%) are `var`.

  So Ljós's fast tier must **infer types from declarations and metadata and handle dynamic `Value`s**, rather than requiring annotations.
- **Modern QML moves name resolution to compile time.** `pragma ComponentBehavior: Bound` (6.4), required properties on delegates, `final` (6.10) and `virtual`/`override` (6.11) all point that way. HAL-C2 already uses `Bound` in 61 of 194 files and has 342 `required property` declarations. qml-solid's `scope.rs`/`resolve.rs` already resolves names statically. Ljós should treat static resolution as the norm and keep dynamic lookup as a fallback that only the JS tier uses.
- **rquickjs 0.14.0 (2026-09-18, built on quickjs-ng) gives us what we need:**
  - `#[class]`, `#[methods]` and `#[exotic]`.
  - `JsClass::exotic_get_property` and the related exotic hooks.
  - `Accessor::new(get, set)` for prototype getters and setters.
  - `Persistent` handles.
  - `AsyncRuntime` behind the `futures` feature.

  Objects should get **per-type prototypes with accessor properties**, which keeps shapes stable for QuickJS inline caches. Exotic objects should be reserved for dynamic cases such as `model` role objects.
- **No existing reactive crate fits as-is:**
  - `reactive_graph` 0.2.15 uses the Reactively algorithm, which is good. However, its effects run as *async tasks on the next tick* via `any_spawner`, which breaks synchronous handlers.
  - `sycamore-reactive` 0.9.4 is closure-per-node.
  - `futures-signals` 0.3.34 is stream-based and has had no release since 2024-07.
  - `salsa` 0.28.5 is built for compilers.
  - **Recommendation:** write a small purpose-built graph (`ljos_reactive`) on `slotmap` 1.1.1, keyed by `(ObjectId, PropIndex)`, with lazily allocated nodes. Use salsa in the *compiler/LSP*, not in the runtime.
- **HAL-C2 usage defines the first subset:**

  | Usage | Count |
  |---|---|
  | `Layout.*` uses | 1414 |
  | `modelData` uses | 1234 |
  | `anchors.*` uses | 803 |
  | `Label` | 320 |
  | `RowLayout` / `ColumnLayout` | 226 / 169 |
  | `Text` | 201 |
  | `Repeater` | 142 |
  | `Component` | 108 |
  | inline `component` declarations | 90 |
  | `Behavior on` | 45 |
  | `TapHandler` / `HoverHandler` | 40 / 36 |
  | `Loader` | 28 |
  | `ListView` / `Connections` | 23 / 23 |
  | `Qt.binding` | 18 |
  | `Instantiator` | 9 |
  | **`states` / `State` / `Binding {}`** | **0** |
  | `ShaderEffect` / `layer` | 1 |

  Imports: `QtQuick.Controls.Basic` is imported by 85 files and `Material` by 10.

## 2. Detailed findings

### 2.1 Object and property system

**Property declarations.** The full Qt 6.12 grammar is `[default] [virtual] [override] [final] [required] [readonly] property <type> <name>`.
- **Change signals.** Every declared property gets an implicit `<name>Changed` signal and an `on<Name>Changed` handler.
- **`default`.** There is at most one default property per object definition. Unnamed child objects are assigned to it.
- **`required`.** A required property must be set at creation. A static violation stops the application from starting. Dynamic creation (`createObject`/`Qt.createComponent`) produces a warning and returns `null`. An inherited property can be made required with `required <name>`.
- **`readonly`.** The property must be initialised, either to a static value or a binding, and later writes throw a `TypeError`. A readonly *binding* is still live.
- **Shadowing.** By default a derived type that redeclares a property creates *two* properties, and which one you get depends on scope. `final` (6.10) forbids shadowing and overriding. `virtual`/`override` (6.11) make the intent explicit, and overriding a non-virtual property is an error. qmlsc `--static` "generates invalid code if any properties are shadowed". **Ljós decision:** resolve property indices per *declaring type*, give each shadowing declaration its own slot, and choose the slot from the static type at the access site. `final` lets the compiler skip the shadow check.

**Aliases.** `property alias name: ref` has these restrictions:
- The reference must be an `id` or `id.prop` within the enclosing component.
- It cannot be an arbitrary expression, an attached property, or a declared type.
- It cannot go to depth 3 or more. `rect.border.color` is allowed (2 levels).

An alias is not a binding; it forwards reads and writes. Its type is the *declared* type of the target, so properties added inline on the target are not reachable through the alias. Undocumented edges that implementations get wrong:
- (a) **Alias to an alias.** Resolve the chain at component link time and detect cycles.
- (b) **Alias activation timing.** Aliases become usable only once the component's ids exist. Reading one during construction, from a binding on an outer object, must work because ids are allocated before any binding runs. qml-solid hits this with its `pending` proxy.
- (c) **Notifications.** An alias forwards the target's notify signal, so `onAliasChanged` must fire when the target changes.
- (d) **Default aliases.** `default property alias content: inner.data` is the idiom for "slot" containers and is very common.

**Value types** are `point`, `size`, `rect`, `color`, `font`, `url`, `vector2d/3d/4d`, `quaternion`, `matrix4x4`, `date`, `real`, `int`, `bool`, `string`, `var` and `list<T>`.
- **Grouped sub-property bindings.** `font.pixelSize: 12; font.bold: hovered` places a separate binding on each sub-field. Each one writes into the composite value, and the *whole* property notifies (`fontChanged`).
- **Value-type references.** In JS, `ValueTypeBehavior: Reference` is the default. `item.font` yields a reference that re-reads on access and writes back on assignment, so `item.font.pixelSize = 3` mutates `item`. `Copy` is experimental. `Assertable` (6.8) makes `as` a pure type check.
- **List properties.** Assigning to a list property **appends** by default (`ListPropertyAssignBehavior: Append`). `Replace` and `ReplaceIfNotDefault` are the alternatives.

**Signals and handlers.**
- **Declaration and duplicates.** Signals are declared as `signal name(a: int, b: string)`. Declaring a signal or method with a duplicate name in the same block is an error, while a derived type may hide one.
- **Handler parameters.** Handlers take parameters through arrow syntax, `onActivated: (x, y) => …`, which cannot carry type annotations.
- **Change handlers.** Every property has `on<Prop>Changed`, even when the type's documentation does not list the signal.
- **`Connections`.**
  - Handlers should be written as `function onFoo(args) {}`.
  - If one handler in a `Connections` object uses the old `onFoo:` form, **all** `function` handlers in that object are ignored. Edge cases like this must be reproduced, ideally with a lint warning.
  - `target` defaults to the parent object. A `null` target silently disconnects. With `ignoreUnknownSignals` false, connecting to an unknown signal is a runtime error.
- **Manual emission.** Calling `fooChanged()` from JS emits the notify signal manually. Apps do this after mutating a `var` array in place, which does not notify. **The reactive graph must treat a manual notify emission as a source change.**

**Methods.** Method parameters are `var` unless annotated. Since 6.7, type annotations are *enforced at call time* (`FunctionSignatureBehavior: Enforced`), so a value passed into a typed parameter is coerced or rejected.

**Attached properties and attached objects.**
- Each attaching type has one attached object per attachee. The engine calls `qmlAttachedProperties` "at most once … for each attachee", and the attached object lives until the attachee is destroyed.
- Attached objects are created on first use. An attached signal handler (`Component.onCompleted`, `Keys.onPressed`, `ListView.onPooled`) forces creation at load time.
- **Classic pitfall:** an *unqualified* attached read always resolves on the *scope object*. Inside a ListView delegate, a child `Rectangle` that reads `ListView.isCurrentItem` gets its own attached object, which no view populates, so the value is false. The fix is `wrapper.ListView.isCurrentItem`. Ljós must reproduce this exactly, because apps work around it.

**Enums.** Enum types and values start with an uppercase letter. They are accessed as `Type.Value` or `Type.Enum.Value` and are stored as integers.

**ids.**
- An object has at most one id. It starts with a lowercase letter or `_` and cannot change.
- An id is not a property (`obj.id` is invalid).
- An id is valid only within its document. Reaching another document's root id works only through dynamic scoping, which the docs call fragile, and internal ids are a `ReferenceError` from outside.

**Scope chain (unqualified names).** From the scope docs:
1. The JS local variable object.
2. **ids** in the component scope, which "always take precedence over other property names".
3. Properties of the binding's **scope object**.
4. Properties of the component **root**.
5. **Ancestor component contexts**, which is dynamic scoping. The nearer component wins.
6. The **JS global** object, which is read-only. QML forbids names that clash with globals, so `Math` cannot be shadowed.

Imports, singletons and JS namespaces apply document-wide. In practice they are resolved before the global object.

`ComponentBehavior` changes this order:
- **`Unbound` (the default).** Local properties override outer ids, but not ids defined inside the component.
- **`Bound`.** Outer ids override local properties. Components can create objects only in their original context, and delegates get **no** private context, so model data must arrive through required properties.

`this` is defined only in bindings, handlers and QML functions, where it refers to the scope object. Inside a `Qt.binding` function, `this` is the receiving object.

**Ownership and parenting.**
- There are two trees. The QObject ownership tree (`data`/`resources`) and the visual tree (`parent`/`children`) are distinct.
  - Children declared inline land in `data`.
  - Items go to `children`, and non-Items such as `Timer` and `QtObject` go to `resources`.
  - Setting `item.parent` changes only the visual parent.
- Ownership examples:
  - A Loader owns its `item`.
  - Repeater delegates are visually parented to the Repeater's *parent*.
  - ListView delegates are parented to `contentItem`, and that parent "can change or be null", so delegates should not bind to `parent.width`.
- Objects from `createObject` without a parent are **JS-owned** and garbage collected when unreferenced.
- `obj.destroy()` is **deferred**: deletion happens after the current JS returns. `destroy(ms)` adds a delay.

**Inline components and singletons.**
- `component Foo: Rect {…}` defines a named sub-component within the file. It shares the file's imports and `pragma ComponentBehavior` but has its own id scope. HAL-C2 has 90 of them, so they are first-class.
- Singletons are declared with `pragma Singleton` plus a qmldir entry, or `QML_SINGLETON` in C++. There is one instance per engine, created on first access.
- `pragma Translator` (6.7) sets the translation context for the file.

**Lifecycle.**
- *Initial properties* are applied "before the object creation is finalized". This covers required properties, the `createObject(parent, props)` map and `Loader.setSource(url, props)`. Initial values set through `setSource` do not trigger Behaviors.
- For C++ types, `QQmlParserStatus::componentComplete()` runs after property initialisation.
- `Component.onCompleted` and `onDestruction` run per object, and "the order of running the handlers is undefined." Real apps nevertheless depend on the *actual* order, so it must be measured against real Qt with the qml-solid gallery harness.
- The creation context of a `Component` is where it was *declared*, not where it is instantiated. A Loader whose `sourceComponent` is defined outside a delegate therefore cannot see that delegate's `index`.

**Binding manipulation.**
- **Imperative assignment** (`x = 5` in JS) removes the binding. Qt can log this through the `qt.qml.binding.removal.info` category, and Ljós should have an equivalent diagnostic.
- **`Qt.binding(fn)`** installs a new binding instead. It cannot be used inside declarative bindings.
- **`Binding {}`** has `target`, `property`, `value`, `when`, `delayed` and `restoreMode`.
  - The default `restoreMode` is `RestoreBindingOrValue`, so disabling the `Binding` restores whatever was there, including the previous *binding*.
  - The generalized grouped form `Binding { t1.text: …; t2.text: … }` evaluates the expression only while enabled.
  - With `target`/`property`/`value`, `value` is evaluated **even when `when` is false**.
  - `delayed: true` defers the write until the event queue clears.
- **`PropertyChanges`** in states creates live bindings by default. `explicit: true` evaluates once, `restoreEntryValues` defaults to true, and `undefined` resets. **Ljós therefore needs a per-property override stack** with entries for the base value or binding, active `Binding` elements, and active state changes.

**Behavior.**
- A `Behavior` is a write *interceptor*. A write sets `targetValue`, and the animation moves the stored value towards it.
- There can be one Behavior per property, and a matching Transition overrides it.
- In practice it does not trigger during the initial creation pass. This should be verified against Qt.

**`Qt.callLater(fn, …args)`** runs the call after control returns to the event loop. Calls are deduplicated by *function identity*, and the last argument set wins.

### 2.2 Binding evaluation in Qt

**Dependency capture.** Dependency tracking is dynamic. Every notifying property read during evaluation is captured, including reads inside called JS functions, and the set is re-captured on every run, so conditional branches change the dependencies.
- **Not tracked:**
  - Plain JS variables in `.js` libraries.
  - `Date.now()`.
  - Mutations of `var` contents.
  - Non-notifying C++ properties.
- **Evaluation order is "not guaranteed".**
- **Binding loops.** When a binding is re-triggered while it is being evaluated, Qt prints `QML <Type>: Binding loop detected for property "<name>"` and does not re-enter. The Qt forum describes the usual cause as circular bindings that keep changing the value within one event-loop cycle.

**Qt 6 bindable properties (C++ side).**
- `QProperty<T>` holds both the value and a pointer to binding data. `QObjectBindableProperty` stores the binding pointer in the owning QObject.
- `setValue` removes the binding, and `takeBinding` detaches it.
- `onValueChanged` and `addNotifier` register observers. `subscribe` also fires once immediately.
- **Update groups** are begun and ended with `Qt::begin/endPropertyUpdateGroup`, and `QScopedPropertyUpdateGroup` (6.6) is the RAII form. Inside a group, writes "neither immediately update any dependent properties nor … trigger change notifications". Groups nest, and notifications are sent only after *all* affected values are updated, so class invariants can be restored first. Reads inside a group return *old* derived values (the 6.2 blog example reads 3, then gets one notification with 5).
- Rules that carry over to Ljós's Rust-defined types:
  - A setter must always write.
  - Don't use a property as a scratch variable.
  - Don't write while invariants are broken.
  - A binding must not read or write its own property.
  - Properties are single-threaded only.
- The history is the useful lesson. 6.0 was lazy; 6.2 is eager plus groups. The stated reasons were the porting cost, since "probably as much as 50%" of properties needed immediate evaluation, and the render thread: lazy bindings "would … get evaluated during that sync period and from the wrong thread".

**AOT (qmlcachegen/qmlsc).**
- qmlcachegen compiles all JS to bytecode. Native C++ is generated only for code that can be "exhaustively analyzed": QObject property lookups through the meta-object, arithmetic, `if/else` and loops. Anything else falls back to bytecode or the JIT for *that whole function*.
- Per-function success and failure statistics are emitted through `QT_QML_GENERATE_AOTSTATS` (6.8.3+).
- `--static` assumes no shadowing. `--direct-calls` calls getters directly but links private APIs.
- Several things defeat compilation: unqualified access, context properties (which the model/view docs say "block Qt Quick Compiler optimizations"), untyped `var` and unbound components. The real lesson is that **compilation hinges on static name resolution and known property types, not on syntax**.

**Deferred properties.** Controls defers `contentItem` and `background` so that a customised control does not instantiate the style's default item as well. That deferral only works if the style's default item has **no `id`**. Since 5.15, replaced items are no longer deleted.

### 2.3 Component creation and models

**`Component`.**
- `createObject(parent, props)` returns `null` on failure.
- `incubateObject(parent, props, mode)` is asynchronous by default. Its incubator exposes `status`, `object` and `forceCompletion()`.
- `Component.status` is one of `Null`, `Ready`, `Loading` or `Error`.
- `Qt.createComponent(url | module, type, mode, parent)` takes a mode. `PreferSynchronous` is the default, and `Asynchronous` loads on a background thread but is a no-op for C++ types and inline components.
- `Qt.createQmlObject` is "extremely slow" because it compiles every time.

**`Loader`.**
- **Activation.** `active` defaults to true. An inactive Loader keeps its `source`/`sourceComponent` and caches `setSource` properties. Changing the source destroys the previous item.
- **Sizing.** With no explicit size, the Loader sizes to the item. With an explicit size or anchors, the item is sized to the Loader.
- **Focus.** A Loader is a FocusScope.
- **`asynchronous: true`.** Objects are created "across multiple frames". Setting it back to false mid-load forces completion.

**Repeater, Instantiator and views.** Repeater instantiates Items, and Instantiator instantiates any object.
- **Delegate roles.**
  - With required properties, roles bind by name, and `model`, `index` and `modelData` are available only if declared.
  - Without required properties, roles are injected as context properties.
  - Integer models expose only `index`, with a limit of 1e8.
  - JS arrays of scalars expose `modelData`, and arrays of objects expose `model` and `modelData` structurally.
  - With a single named role, `modelData` equals that role.
  - `index` becomes -1 when the item is removed.
- **Writing back.** `delegateModelAccess` (6.10) defaults to `Qt5ReadWrite`. Under that mode, writing to a *required property* breaks its binding to the model rather than writing through, and writes via `model.x` are needed. `ReadWrite` propagates required-property writes to the model. Writing to JS-array or int models never propagates.

**ListView.**
- **Delegate lifetime.** Delegates are created on demand and "may be destroyed at any time", so state must live in the model.
- **`cacheBuffer`.** It is measured in pixels and has a platform-dependent default. Buffered delegates are created *asynchronously across frames* and not painted.
- **`displayMarginBeginning/End`.** These margins are for layout, not performance.
- **`reuseItems`** defaults to false.
  - An item enters the pool only when *fully* outside the view, and `currentItem` is never pooled.
  - `pooled` and `reused` attached signals fire on the transitions.
  - Model-provided properties are refreshed on reuse, but other state is not.
  - Pooled items "might still be alive and respond to connected signals and bindings".
- **Attached properties.** `ListView.delayRemove` holds destruction until a remove transition finishes.

### 2.4 Qt Quick item tree essentials (what has semantic weight)

- **Geometry.** Item geometry is `x/y/width/height`, with `implicitWidth/Height` as the item's natural size. When no explicit size is set, `width` follows `implicitWidth`, which qml-solid implements as a "derived default". Other properties: `z`, `clip`, and `visible`, `enabled` and `opacity`, which are *effective* values combined down the tree, so a child's `visible` reads false when an ancestor is hidden.
- **Anchors and positioners are not bindings.** They are imperative geometry managers that react to geometry changes. Writing `x` on an anchored item conflicts with the anchor. Anchors cover `fill`, `centerIn`, margins, `baseline` and `alignWhenCentered`.
- **Qt Quick Layouts** run in a deferred *polish* phase.
  - If minimum is unset it is the implicit minimum: 0 for non-layouts.
  - Maximum defaults to infinity.
  - `preferredWidth: -1` means use `implicitWidth`.
  - `fillWidth` defaults by size policy and is true for nested layouts.
  - Stretch factors (6.5) distribute extra space between preferred and maximum and require `fill`.
  - Default `alignment` 0 means `AlignVCenter|AlignLeft`.
  - Margins are applied by the parent layout.
  - `useDefaultSizePolicy` (6.8) controls whether the item's implicit size policy is used.
- **Animation.** Animators (`XAnimator`, `OpacityAnimator` and others) run on the render thread. "The property is not updated while the animation is running", and they have "no benefit when a non-threaded render loop is used". Their default duration is 250 ms.
- **Popups.**
  - A Popup's content is reparented to `Overlay.overlay`.
  - Popups with equal `z` stack by open order.
  - `closePolicy` defaults to `CloseOnEscape|CloseOnPressOutside`.
  - `dim` follows `modal`.
  - `popupType` is one of `Item`, `Window` or `Native` (6.8), each falling back to the next simpler kind.
  - Enter and exit transitions restore `opacity`/`scale` after exit.
- **ShaderEffect and `layer.enabled`.** These can be deferred, since HAL-C2 uses them once.

### 2.5 Qt Quick Controls architecture

- **Templates and styles.** `QtQuick.Templates` holds behaviour-only C++ types, and a style is a set of QML files, one per control, each rooted in the `T.` template type.
  - Importing `QtQuick.Controls` from within a style makes the control derive from itself.
  - Controls a style does not implement fall back to Basic.
  - Default styles per platform: Android uses Material, iOS uses iOS, Linux uses Fusion, macOS and Windows use their native styles, and other platforms use Basic.
  - Compile-time style selection (`import QtQuick.Controls.Material` first) overrides run-time selection, which is chosen through `QQuickStyle::setStyle`, `-style`, `QT_QUICK_CONTROLS_STYLE` or `qtquickcontrols2.conf`.
- **Sizing formula.** Every template uses `implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset, implicitContentWidth + leftPadding + rightPadding)`.
  - Padding cascades: `padding` → `horizontalPadding` → `left/rightPadding`.
  - `implicitContentWidth` is `contentItem.implicitWidth`, or is computed from content children for `Pane`/`Container`.
  - The background fills the control unless it has insets or an explicit size.
- **Propagation.** `font`, `palette`, `locale` and `hoverEnabled` propagate from parent controls and windows to children unless explicitly set. This is *resolve-mask inheritance*: only explicitly set sub-fields propagate. `mirrored` is driven by `LayoutMirroring`.
- **Clean-room consequence:** Qt's style QML files are Qt *source code*, so Ljós must author its own Basic and Material style QML from the documentation and from behavioural/visual comparison in the gallery, never by copying.

## 3. Recommendations for Ljós

### 3.1 Execution model: one single-threaded Engine per "UI process"

- **Decision.** An `Engine` is `!Send`. It owns the object arena, reactive graph, QuickJS `Runtime`/`Context`, type registry and scheduler for one window (later, one isolated subtree).
- **BEAM analogues:**
  - Each Engine has a **reduction budget per frame**, counted in object creations, binding evaluations and JS interrupt-handler ticks.
  - A **supervisor** can tear down and re-instantiate a component subtree whose bindings throw repeatedly or exceed the budget.
  - Cross-engine communication is by message passing (egui_mobius-style channels), never by shared bindings.
  - tokio I/O results arrive as messages that are drained at the turn boundary.
- **Rejected:** a `Send+Sync` graph, which is what `reactive_graph` provides with `Arc` and `RwLock`. Its locks cost every read for no benefit, and QML semantics are single-threaded anyway; Qt's bindable properties are single-thread-only too.

### 3.2 Object store

```rust
slotmap::new_key_type! {
    pub struct ObjectId; pub struct BindingId; pub struct NodeId; pub struct ContextId;
}
#[derive(Copy, Clone, PartialEq, Eq, Hash)] pub struct PropIndex(pub u16);   // per *declaring* type slot
#[derive(Copy, Clone, PartialEq, Eq, Hash)] pub struct PropKey { pub obj: ObjectId, pub prop: PropIndex }

pub struct ObjectHeader {
    pub meta: &'static MetaObject,          // or Arc<MetaObject> for QML-defined types (hot reload)
    pub native: Box<dyn QmlObject>,         // Rust-declared fields (Item geometry, Text, …)
    pub dynamic: Box<[Value]>,              // QML-declared `property` slots, indexed after native ones
    pub owner: Option<ObjectId>,            // QObject-parent (ownership)
    pub visual_parent: Option<ObjectId>,    // Item parent (may differ!)
    pub context: ContextId,                 // creation context (ids, outer scope)
    pub attached: SmallVec<[(AttachedTypeId, ObjectId); 1]>, // lazily created
    pub js: Option<rquickjs::Persistent<rquickjs::Object<'static>>>, // lazily created wrapper
    pub flags: ObjectFlags,                 // COMPLETED, JS_OWNED, PENDING_DELETE, POOLED, …
}

#[repr(C)] pub enum Value {               // target: 16 bytes
    Undefined, Null, Bool(bool), Int(i32), Real(f64), String(Atom /* interned Rc<str> */),
    Color(Rgba8 /* or packed f16x4 */), Point(Box<[f64; 2]>) /* or inline f32x2 */, Size(..), Rect(Box<RectF>),
    Font(Rc<Font>), Url(Rc<Url>), Object(ObjectId), List(Rc<ListValue>), Enum(i32),
    Js(JsHandle /* index into a Persistent table for `var` holding JS objects */),
}
```

- **Arena.** Use `slotmap::SlotMap` for objects. Generational keys turn use-after-destroy into a detectable `None`, which JS sees as `null`/`TypeError`. `SecondaryMap` holds sparse side tables: override stacks, JS wrappers, attached objects for rare cases.
- **Rejected:** `generational-arena` (last release 2023, unmaintained), `thunderdome` (last release 2023) and `hecs` (an ECS fragments per-object property access).
- **Typed native storage.** Rust-defined types keep real typed fields (`f64 x`, `Color color`), accessed through generated `fn(&dyn Any, PropIndex) -> Value` getters plus typed fast-path accessors for the renderer and layout. QML-declared properties use `dynamic: [Value]`.
- **Rejected:** all-dynamic storage, a `HashMap<Atom, Value>` per object. It is slower and bigger, and it defeats static resolution.

### 3.3 Reactive graph (`ljos_reactive`): lazily allocated, three-colour push-pull

```rust
pub struct Graph {
    nodes: SlotMap<NodeId, Node>,
    by_prop: FxHashMap<PropKey, NodeId>,     // node exists only once a property is observed or bound
    effects_queue: BinaryHeap<(Reverse<u32 /*height*/>, NodeId)>,
    batch_depth: u32, epoch: u64, tracking: Option<Tracker>,
}
pub struct Node {
    kind: NodeKind, state: NodeState, height: u32, changed_at: u64,
    sources: SmallVec<[NodeId; 4]>, observers: SmallVec<[NodeId; 4]>,
}
pub enum NodeKind { Source(PropKey), Binding { target: PropKey, binding: BindingId }, Effect(EffectId) }
pub enum NodeState { Clean, Check, Dirty, Computing }
pub struct Tracker { node: NodeId, reads: SmallVec<[NodeId; 8]> }

pub trait Reactive {
    fn read(&self, key: PropKey) -> Value;                       // captures if tracking; pulls if Check/Dirty
    fn write(&self, key: PropKey, v: Value, how: WriteKind) -> Result<(), QmlError>; // Imperative breaks binding
    fn notify(&self, key: PropKey);                              // manual `fooChanged()` emission
    fn batch<R>(&self, f: impl FnOnce() -> R) -> R;              // = Qt property update group
}
pub enum WriteKind { Imperative /*breaks binding*/, FromBinding, Initial /*no Behavior*/, Override(OverrideId) }
```

- **Writes.** A write marks direct observers `Dirty` and transitive observers `Check`, in the style of Reactively/Preact. Equal values do not propagate; a manual `notify` always does. It then **synchronously flushes effects** in height order before returning to JS, unless inside `batch`. This reproduces Qt's synchronous `onXChanged` and the guard-flag idiom.
- **Effect roots.** These are the only eagerly evaluated consumers:
  - signal handlers (`onXChanged`, `Connections`);
  - Behavior interceptors;
  - Rust subscribers;
  - anchor, positioner and layout managers, which become *polish* effects queued for the frame;
  - accessibility observers, which exist only while an AT is attached.
- **Lazy render bindings.** Everything else, such as `color: hovered ? a : b` on an off-screen delegate, stays dirty until something reads it. Paint reads only visible items. This is the main **off-screen resource win**, and it is safe because Ljós has no render-thread sync (Qt's reason for abandoning laziness). Only side-effecting bindings can observe the difference, and the docs already discourage relying on evaluation order.
- **Glitch-freedom within a flush** comes from height ordering plus `Check`-state verification against `changed_at`. Consecutive *unbatched* writes still produce intermediate notifications, exactly like Qt. Internally, `batch` wraps component creation, model resets, layout and animation ticks.
- **Loop detection.**
  - Entering a node that is `Computing` is a binding loop. Ljós logs Qt's message text, returns the stale value and does not re-enter.
  - Handler ping-pong is caught by a per-flush re-dirty cap (100 per node).
  - Polish passes are capped at 10 per frame, with a warning.
- **Dynamic re-capture.** Each evaluation diffs the new read set against `sources`, unsubscribing dead branches with an epoch stamp rather than a set rebuild.
- **Override stack for `Binding`/`State`.** `SecondaryMap<PropKey, OverrideStack>` holds the base (value or binding) plus ordered overrides. `restoreMode` and `restoreEntryValues` pop it.
- **Value-type sub-bindings.** `font.pixelSize` is a binding node whose target is `(key, SubField::PixelSize)`. It writes the field and notifies the parent key.
- **Translations.** `qsTr` reads a global `ui_language` source node, so `Qt.uiLanguage = "is"` bumps one node and re-evaluates exactly the translated bindings.

**Crate evaluation (versions as of 2026-10):**

| Crate | Version / date | Verdict |
|---|---|---|
| `slotmap` | 1.1.1 (2025-12) | **Use.** Arena, generational keys and SecondaryMap. |
| `reactive_graph` | 0.2.15 (2026-09) | Study only. It uses the Reactively algorithm, which is good, but effects are async tasks on `any_spawner` "until the next tick", it is Arc/RwLock-based, and it has no keyed external sources or binding-breaking. |
| `leptos` | 0.8.22 | Not applicable: it is a whole framework. |
| `sycamore-reactive` | 0.9.4 (2026-10) | Study only. It has single-threaded arena signals and synchronous effects, but it is closure-per-node, which is too heavy per property. |
| `futures-signals` | 0.3.34 (2024-07) | Reject. It is stream-based and lossy, and it is stale. |
| `salsa` | 0.28.5 (2026-09) | **Use in the compiler/LSP**, for incremental re-analysis and hot reload diffing, but not at runtime. |
| `generational-arena` / `thunderdome` | 0.2.9 (2023) / 0.6.1 (2023) | Reject: unmaintained. |

### 3.4 Three-tier binding execution

**Tier 0 is static resolution in the compiler**, reusing qml-solid's oxc QML parser and adapting `scope.rs`/`resolve.rs`. Every identifier in a binding becomes one of these:

```rust
pub enum Resolved {
    Local(u16), Id(IdSlot), ScopeProp(PropIndex), RootProp(PropIndex), OuterId { depth: u8, slot: IdSlot },
    Attached { ty: AttachedTypeId, on: ObjRef }, Enum(i32), Singleton(TypeId), ImportNs(NsId),
    Global(GlobalId), ContextProp(Atom) /* Unbound delegate roles, dynamic */, Unknown(Atom),
}
```

**Tier 1 is the Ljós binding IR**: a register bytecode over `Value`, interpreted in Rust without entering QuickJS.
- **Subset:**
  - literals, resolved reads and writes, value-type fields;
  - `+ - * / % ** ?: && || ?? ! == === < …` with exact JS coercion rules;
  - template strings;
  - calls to a whitelist of pure builtins: `Math.*`, `qsTr`, `Qt.rgba/point/size/rect`, `String#arg`, a `toFixed` subset;
  - user QML functions whose bodies are themselves in the subset, which get inlined.
- **No type annotations required.** The IR is dynamically typed, with specialised opcodes when declared or metadata types are known (`AddRealReal`). This matters because HAL-C2 is 31% `var` and functions are almost entirely unannotated. With annotations this would be qmlsc territory; without them it resembles QuickJS's own bytecode specialised for QML.
- **Opcodes** look like `LoadProp{dst, obj: ObjRef, prop}`. Each read calls `Graph::read`, so dependency capture is identical across tiers.
- **Binding size.** An IR binding is `Arc<[Op]>` plus a constant pool. It is shared by all instances of a component, so per-instance cost is one `BindingId` plus a node, allocated on first evaluation.
- **Expected coverage.** Typical QML is dominated by bindings like `parent.width - 2*margin`, `hovered ? Theme.a : Theme.b` and `qsTr("…")`, and Tier 1 should cover the majority. **This must be measured**, by compiling the HAL-C2 corpus and reporting tier-1 coverage, just as Qt's aotstats does.

**Tier 2 is rquickjs** (0.14, quickjs-ng) for everything else: closures, loops, arrays, `JSON`, regex, user JS libraries, handlers that mutate.
- **Compilation.** Each QML document compiles to one JS module (oxc codegen, as qml-solid does), whose binding functions take resolved references as parameters, for example `($scope, $ids) => $ids.list.count > 0 ? … : …`. There is no `with`-style lookup.
- **Object wrappers.** Each QML type gets a **prototype built once with `Accessor::new(getter, setter)` per property**. The getter extracts the `ObjectId` from the `#[class]` instance and calls `Graph::read`, reaching the Engine through context userdata or a thread-local. Methods and signals are also prototype functions.
- **Why not exotic objects.** Plain prototypes keep QuickJS shapes stable for inline caches. `#[exotic]` (`exotic_get_property`) is reserved for dynamic things: Unbound-delegate `model` role objects, `ListModel` element proxies and context-property fallbacks.
- **Wrapper lifetime.** One wrapper per object, created lazily and cached as a `Persistent`. JS-owned objects, those created with `createObject` and no parent, are destroyed by the wrapper's finalizer. Destroyed objects read as `null`.
- **Watchdog.** QuickJS's interrupt handler enforces the reduction budget, and a runtime memory limit is set per Engine.
- **Debug mode cross-checks tiers.** Every Tier-1 binding is also evaluated in Tier 2 and the results compared. This is the cheapest way to keep the hand-written JS semantics in Tier 1 honest.

**Rejected alternatives:**
- **Everything in QuickJS.** Each binding would enter the engine and allocate, and the GC would have to run on mobile.
- **A Cranelift JIT.** iOS forbids JIT, and the gain over a tight interpreter is small for one-line bindings.
- **Typed-only AOT to Rust source**, as qmlsc does. That would require annotations HAL-C2 lacks, and it fights hot reload.

### 3.5 Creation, incubation and the frame loop

- **Creation phases.** Creation is a resumable state machine, the `Incubator`, with these phases:
  1. Allocate every object and register ids in the context.
  2. Apply initial and required properties, then literal values, inside one `batch`. Missing required properties are an error.
  3. Install bindings. They start dirty and are pulled lazily, except effect roots.
  4. Run Rust `component_complete()`.
  5. Attach Behaviors.
  6. Run `Component.onCompleted`, in a fixed deterministic order measured against Qt.
- **Budgeting.** Each step charges reductions.
  - Synchronous creation runs to completion.
  - `Loader.asynchronous`, `incubateObject` and the ListView `cacheBuffer` run with a per-frame budget of about 4 ms at 60 Hz and 2 ms at 120 Hz.
  - `forceCompletion()` drains the remaining work.
- **Frame order:**
  1. Input events. Each handler is a *turn*, followed by an effect flush.
  2. Timers, tokio messages and the `callLater` queue (deduplicated by function identity).
  3. Deferred `destroy()` calls.
  4. Animation tick, inside a batch.
  5. Polish: anchors, positioners and layouts, iterated up to 10 times.
  6. Incubation slice.
  7. Paint, which pulls visible items' bindings and feeds egui.
- **Views.**
  - ListView and GridView keep Qt's `reuseItems: false` default for compatibility.
  - Pooled delegates have their effect roots *suspended*. Qt lets them keep running; Ljós can do better because pull-based bindings stay inert.
  - Delegate roles of Bound or required-property delegates are plain property writes on reuse.
- **`DelegateChooser`.** It selects a delegate per row by role value. Implement it as a view-level `fn(row) -> ComponentId`, since HAL-C2 doesn't use it yet.

### 3.6 Item tree and Controls priorities

- **Phase 1 (enough for HAL-C2):**
  - Item, Rectangle, Text/Label (the Text family via parley), Image (async, `sourceSize`, cache).
  - Row/Column/Flow, Row/ColumnLayout, anchors.
  - Repeater, Instantiator, Loader, Component, inline components.
  - TapHandler/HoverHandler/DragHandler and MouseArea.
  - Flickable, ListView, Timer, Connections, QtObject, ListModel.
  - NumberAnimation/ColorAnimation and Behavior.
  - Controls: Control, AbstractButton, Button, Switch, CheckBox, TextField, TextArea, ComboBox, Popup, Dialog, Menu/MenuItem, ScrollBar/ScrollView, Pane, ApplicationWindow, with a **Basic** style written clean-room.
  - `Shortcut`, FocusScope and Keys.
- **Phase 2:** states and transitions, the `Binding` element (unused by HAL-C2, but needed for compatibility), GridView, Drawer, StackView/SwipeView, the Material style (the Android default) and Shapes (ShapePath/PathSvg are used about 9 times).
- **Deferred:** ShaderEffect, `layer.*`, PathView and Animators as distinct types. Animators can alias to ordinary animations, with the "property updated at end" semantics emulated.

### 3.7 Rust type registration (`QML_ELEMENT`/`Q_PROPERTY` equivalent)

```rust
#[derive(QmlObject)]
#[qml(module = "QtQuick", name = "Rectangle", extends = Item, default = "data")]
pub struct Rectangle {
    #[qml(notify)]                 color: Color,
    #[qml(notify)]                 radius: f64,
    #[qml(group)]                  border: Border,           // grouped value type → `border.width`
    #[qml(readonly, notify)]       painted_width: f64,       // exposed as `paintedWidth`
    #[qml(skip)]                   cache: PaintCache,
}

#[qml_impl]
impl Rectangle {
    #[qml(signal)]   fn pressed(&self, x: f64, y: f64);       // body generated (emit)
    #[qml(invokable)] fn contains_point(&self, p: Point) -> bool { … }
    #[qml(on_complete)] fn complete(&mut self, cx: &mut Cx) { … }   // QQmlParserStatus
}

#[derive(QmlAttached)] #[qml(attaches_to = "ListView")]      // created lazily, once per attachee
pub struct ListViewAttached { #[qml(readonly, notify)] is_current_item: bool, #[qml(notify)] delay_remove: bool }

#[derive(QmlValueType)] #[qml(name = "font")] pub struct Font { pixel_size: i32, bold: bool, family: Atom, … }
#[derive(QmlEnum)] pub enum FillMode { Stretch, PreserveAspectFit, … }
#[qml_singleton] pub struct Theme { … }
```

- **What the derive generates.** `impl QmlObject` gives a `&'static MetaObject`, which contains:
  - property table entries: name atom, `QmlType`, flags `READONLY|REQUIRED|FINAL|CONSTANT|DEFAULT|GROUP`, notify signal index, and `get`/`set` fn pointers;
  - typed accessors for render fast paths;
  - signal and method tables, with JS-callable thunks;
  - an attached-type id;
  - `final` by default for Rust properties. This lets the compiler skip shadow checks, which is what qmlsc `--static` assumes globally.
- **Setters.** Generated setters always write and call `Graph::write`, which follows the QProperty rules.
- **Registration.** Use an explicit `ljos::module! { "QtQuick" 2.15 => [Item, Rectangle, …] }` rather than `inventory`/linker sections. Explicit lists are deterministic for static iOS linking and hot reload.
- **Type dumps.** The macro also dumps a `ljostypes.json`, analogous to `.qmltypes`, consumed by the compiler and LSP (salsa). qml-solid's `qmltypes` crate already parses Qt's `.qmltypes`, which is useful for *API surface* checks only. That data is metadata, not code, but the clean-room boundary should be confirmed for it.
- **Rejected:** runtime reflection (`bevy_reflect`-style), which is too slow for hot paths and opaque to the compiler; and hand-written vtables, which are error-prone.

### 3.8 Hot code update hook-ins

Bindings are data: a `BindingId` points to an IR or JS chunk. Reloading a file therefore works as follows:
1. Recompile (salsa) and diff the object tree by `(component path, id or structural position)`.
2. Swap binding code and reattach handlers in place, which preserves imperative state.
3. Re-instantiate only structurally changed subtrees, through the supervisor.

`MetaObject` for QML-defined types must therefore be `Arc`, not `'static`.

## 4. Risks and open questions

1. **Re-entrancy in Rust.** A read can pull a binding, which calls JS, which calls back into Rust reads and writes. The Engine must never hold a `RefCell` borrow across a JS call. Split the state into separately borrowed pieces (graph, objects, JS) and design the API around short borrows.
2. **Lazy versus eager divergence.** Bindings with side effects (logging, writes to other properties, `Date.now()`) behave differently when not pulled. **Mitigations:**
   - A `--eager` debug mode for differential testing against Qt.
   - Automatically promote any binding that *writes* during evaluation to an effect root.
3. **Undefined orders that apps rely on.** This covers `Component.onCompleted` order, binding order at creation and destruction order. **Mitigation:** build a behaviour corpus that logs these orders under real Qt (the qml-solid gallery runner) and match it.
4. **Tier-1 JS semantic fidelity.** Number formatting, string coercion, `==`, `NaN`/`-0` and integer overflow for `int` properties (Qt truncates or wraps) all need exact matching. **Mitigation:** debug-mode dual execution plus test262-style unit cases.
5. **Unbound delegates and context properties.** These remain common in older QML, given 1234 `modelData` uses. They force Tier 2 and exotic lookups and are slower. Ljós should lint for them and offer an auto-fix to `required property`.
6. **QuickJS on mobile.** rquickjs's target table does not list Android or iOS, so these need the `bindgen` feature and CI validation. iOS has no JIT, but QuickJS is an interpreter, so that is fine. GC pauses on large `var` graphs need measuring.
7. **Value-type reference semantics.** Write-back references in JS (`let f = item.font; f.bold = true`) are subtle. Qt has changed them across 6.4–6.8 (`ValueTypeBehavior`), so Ljós needs to pick the 6.8+ behaviour and test it.
8. **Open: per-window Engine versus a shared Engine.** HAL-C2 desktop windows sharing singletons would need cross-engine singleton proxies. One shared Engine with per-subtree budgets may be simpler. Decide after prototyping.
9. **Open: the exact Behavior initialisation rule** (whether a Behavior fires during creation) and the **binding-loop recovery value**. Both should be verified behaviourally against Qt.
10. **Clean-room risk around style QML.** Ljós styles must be authored from scratch. Keep a policy file and require contributors to attest to it.

## 5. Sources

- QML scope and name resolution: https://doc.qt.io/qt-6/qtqml-documents-scope.html
- Property binding: https://doc.qt.io/qt-6/qtqml-syntax-propertybinding.html
- Object attributes (properties, aliases, signals, attached, enums, ids): https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html
- Property shadowing and override semantics: https://doc.qt.io/qt-6/qtqml-syntax-overridesemantics.html
- What's new in QML tooling for Qt 6.11, part 2 (final/virtual/override history): https://www.qt.io/blog/whats-new-in-qml-tooling-for-qt-6.11-part-2
- Document structure and pragmas (ComponentBehavior, ValueTypeBehavior, ListPropertyAssignBehavior, FunctionSignatureBehavior, Translator): https://doc.qt.io/qt-6/qtqml-documents-structure.html
- Binding type: https://doc.qt.io/qt-6/qml-qtqml-binding.html
- Connections: https://doc.qt.io/qt-6/qml-qtqml-connections.html
- Qt global object (callLater, binding, createComponent, qsTr, uiLanguage): https://doc.qt.io/qt-6/qml-qtqml-qt.html
- JS host environment: https://doc.qt.io/qt-6/qtqml-javascript-hostenvironment.html
- Bindable properties: https://doc.qt.io/qt-6/bindableproperties.html
- QProperty: https://doc.qt.io/qt-6/qproperty.html
- QScopedPropertyUpdateGroup: https://doc.qt.io/qt-6/qscopedpropertyupdategroup.html
- All about property bindings in Qt 6.2: https://www.qt.io/blog/all-about-property-bindings-in-qt-6.2
- [Development] Changes to the property system (lazy evaluation removed): https://lists.qt-project.org/pipermail/development/2021-April/041200.html
- QtCS2021 Qt Property Bindings: https://wiki.qt.io/QtCS2021_-_Qt_Property_Bindings
- QML script compiler (qmlsc): https://doc.qt.io/qt-6/qtqml-qml-script-compiler.html
- QT_QML_GENERATE_AOTSTATS: https://doc.qt.io/qt-6/cmake-variable-qt-qml-generate-aotstats.html
- Qt Quick best practices: https://doc.qt.io/qt-6/qtquick-bestpractices.html
- Defining QML types from C++: https://doc.qt.io/qt-6/qtqml-cppintegration-definetypes.html
- Component: https://doc.qt.io/qt-6/qml-qtqml-component.html
- Loader: https://doc.qt.io/qt-6/qml-qtquick-loader.html
- ListView: https://doc.qt.io/qt-6/qml-qtquick-listview.html
- Models and views (required properties, modelData, delegateModelAccess): https://doc.qt.io/qt-6/qtquick-modelviewsdata-modelview.html
- Layout attached properties: https://doc.qt.io/qt-6/qml-qtquick-layouts-layout.html
- PropertyChanges: https://doc.qt.io/qt-6/qml-qtquick-propertychanges.html
- Behavior: https://doc.qt.io/qt-6/qml-qtquick-behavior.html
- Animator: https://doc.qt.io/qt-6/qml-qtquick-animator.html
- Controls: customizing: https://doc.qt.io/qt-6/qtquickcontrols-customize.html
- Controls: styles: https://doc.qt.io/qt-6/qtquickcontrols-styles.html
- Control (sizing, padding, propagation): https://doc.qt.io/qt-6/qml-qtquick-controls-control.html
- Popup: https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html
- Qt 6.11 release: https://www.qt.io/blog/qt-6.11-released
- Qt forum, binding loop cause (community explanation): https://forum.qt.io/topic/113934/qml-binding-loop-detected-for-property-what-is-the-cause
- rquickjs docs: https://docs.rs/rquickjs/latest/rquickjs/ ; JsClass: https://docs.rs/rquickjs/latest/rquickjs/class/trait.JsClass.html ; exotic: https://docs.rs/rquickjs/latest/rquickjs/attr.exotic.html ; Accessor: https://docs.rs/rquickjs/latest/rquickjs/object/struct.Accessor.html ; repo (quickjs-ng): https://github.com/DelSkayn/rquickjs
- reactive_graph: https://docs.rs/reactive_graph/latest/reactive_graph/
- Crate versions: crates.io API, queried 2026-10-10 (https://crates.io/api/v1/crates/{reactive_graph,leptos,futures-signals,sycamore-reactive,salsa,slotmap,generational-arena,thunderdome,hecs,rquickjs,egui,accesskit,any_spawner})
- Local measurements: `/home/olafura/dev/hal-c2/apps/{desktop-qt,mobile-qt}`, 194 `.qml` files and 43,398 lines (read-only grep counts). qml-solid design: `/home/olafura/dev/qml-solid/README.md`, `packages/runtime/object.js` and `crates/qml_solid/src/scope.rs`.
