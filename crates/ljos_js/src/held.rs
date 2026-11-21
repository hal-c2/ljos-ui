//! What the script heap holds for given values, found by walking what they
//! reach. QuickJS counts its heap by kind only (`JS_ComputeMemoryUsage`),
//! so what holds it is told by a walk, which an inspector asks for when
//! someone looks, never on a timer.

use std::collections::HashSet;

use rquickjs::{Value, qjs};

use crate::JsRef;
use crate::host::{JS_CLASS_ARRAY, JS_CLASS_OBJECT, OwnNames, Scope};

/// [`Scope::held_by`]: about the bytes each group of values holds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Held {
    /// About the bytes reached from each group, in the order the groups
    /// came, each value counted for the first group to reach it. Estimates
    /// of QuickJS's own records (a header, a slot per property, a byte per
    /// character), not the allocator's count.
    pub bytes: Vec<usize>,
    /// The values walked in all.
    pub values: usize,
    /// The walk stopped at its budget, or a read failed, and what was left
    /// was not counted: the bytes are then the least held.
    pub truncated: bool,
}

// About what QuickJS keeps for each: an object's header with its share of
// a shape, a property or array slot, a string's header (a rope's node and
// a slice's too).
const OBJECT: usize = 64;
const SLOT: usize = 16;
const STRING: usize = 16;

unsafe extern "C" {
    // Ljós's patch to the vendored quickjs.c (vendor/QUICKJS.md).
    fn JS_GetOwnPropertyIfMade(
        ctx: *mut qjs::JSContext,
        desc: *mut qjs::JSPropertyDescriptor,
        obj: qjs::JSValue,
        prop: qjs::JSAtom,
    ) -> i32;
    fn JS_GetStringParts(v: qjs::JSValue, a: *mut qjs::JSValue, b: *mut qjs::JSValue) -> i32;
}

/// One walk's state, across all its groups.
struct Walk<'js> {
    seen: HashSet<usize>,
    stack: Vec<Value<'js>>,
    /// What is left of the budget: a value walked and a property read
    /// cost one each, so the work is bounded by it whatever the shape.
    left: usize,
    values: usize,
    cut: bool,
}

impl<'js> Scope<'js> {
    /// About the bytes the values of each group hold in the script heap:
    /// what each reaches through the own properties of ordinary objects
    /// and arrays, counted once. A getter is counted as a function, never
    /// called, so the walk runs no script, and a property QuickJS makes on
    /// its first read is left unmade. A function holds what its closure
    /// holds, which QuickJS does not show: only the function is counted,
    /// and so for a proxy, a host's object and the other kinds whose
    /// insides are the engine's. A reference no longer held is left out.
    /// Stops after `budget` values and property reads in all.
    pub fn held_by<'r, G>(&self, groups: impl IntoIterator<Item = G>, budget: usize) -> Held
    where
        G: IntoIterator<Item = &'r JsRef>,
    {
        let mut walk = Walk {
            seen: HashSet::new(),
            stack: Vec::new(),
            left: budget,
            values: 0,
            cut: false,
        };
        let mut held = Held::default();
        for group in groups {
            let mut bytes = 0;
            for r in group {
                if walk.left == 0 {
                    walk.cut = true;
                    break;
                }
                // A value not made yet (an array a host handed over that no
                // script has read) is not in the heap: making it to look
                // would run the script's hook and grow what is measured.
                if r.key().is_none() {
                    continue;
                }
                match self.restore(r) {
                    Ok(v) => walk.stack.push(v),
                    Err(_) => {
                        let _ = self.ctx().catch();
                    }
                }
                while let Some(v) = walk.stack.pop() {
                    if walk.left == 0 {
                        walk.cut = true;
                        walk.stack.clear();
                        break;
                    }
                    bytes += self.held_at(&v, &mut walk);
                }
            }
            held.bytes.push(bytes);
        }
        held.values = walk.values;
        held.truncated = walk.cut;
        held
    }

    /// What `v` itself holds, its first time; what it refers to goes on
    /// the walk's stack, as many of its properties as the budget has left.
    fn held_at(&self, v: &Value<'js>, walk: &mut Walk<'js>) -> usize {
        let raw = v.as_raw();
        let ctx = self.ctx().as_raw().as_ptr();
        if !(v.is_object() || v.is_string()) {
            // Numbers, booleans and the like live in the slot that holds them.
            return 0;
        }
        // SAFETY: tagged as an object or a string, the payload is the
        // pointer to it, only compared here.
        let at = unsafe { qjs::JS_VALUE_GET_PTR(raw) } as usize;
        if !walk.seen.insert(at) {
            return 0;
        }
        walk.values += 1;
        walk.left -= 1;
        if v.is_string() {
            return self.string_held(raw, walk);
        }
        // SAFETY: `raw` is the live object `v` holds; the class is read off
        // its header.
        let class = unsafe { qjs::JS_GetClassID(raw) };
        if class != JS_CLASS_OBJECT && class != JS_CLASS_ARRAY {
            let mut size = 0;
            // SAFETY: `raw` is live; a buffer's bytes are only measured.
            if unsafe { qjs::JS_IsArrayBuffer(raw) }
                && unsafe { qjs::JS_GetArrayBuffer(ctx, &mut size, raw) }.is_null()
            {
                let _ = self.ctx().catch();
                size = 0;
            }
            return OBJECT + size as usize;
        }
        // Only these two classes: their own properties are plain slots, and
        // listing and reading them runs nothing a script or a host defined.
        let flags =
            (qjs::JS_GPN_STRING_MASK | qjs::JS_GPN_SYMBOL_MASK | qjs::JS_GPN_PRIVATE_MASK) as i32;
        let mut names = OwnNames {
            ctx,
            tab: std::ptr::null_mut(),
            len: 0,
        };
        // SAFETY: the context is entered and `raw` is live; `names` frees
        // what is filled in.
        if unsafe { qjs::JS_GetOwnPropertyNames(ctx, &mut names.tab, &mut names.len, raw, flags) }
            < 0
        {
            let _ = self.ctx().catch();
            walk.cut = true;
            return OBJECT;
        }
        let len = names.len as usize;
        // Each property read is counted against the budget, a number's too:
        // an array of a million numbers is a million reads.
        let reads = len.min(walk.left);
        walk.left -= reads;
        walk.cut |= reads < len;
        for i in 0..reads {
            // SAFETY: `i` is within the table the engine filled in.
            let atom = unsafe { (*names.tab.add(i)).atom };
            let mut d = std::mem::MaybeUninit::<qjs::JSPropertyDescriptor>::uninit();
            // SAFETY: as above. A descriptor found holds owned references
            // to its value, getter and setter, each taken over by a Value.
            unsafe {
                match JS_GetOwnPropertyIfMade(ctx, d.as_mut_ptr(), raw, atom) {
                    1 => {
                        let d = d.assume_init();
                        for part in [d.value, d.getter, d.setter] {
                            let part = Value::from_raw(self.ctx().clone(), part);
                            // Only what can hold more than its slot is walked.
                            if part.is_object() || part.is_string() {
                                walk.stack.push(part);
                            }
                        }
                    }
                    // Gone, or not made yet: only its slot, counted below.
                    0 | 2 => {}
                    _ => {
                        let _ = self.ctx().catch();
                        walk.cut = true;
                    }
                }
            }
        }
        OBJECT + len * SLOT
    }

    /// A rope holds its node and its halves, a slice its header and the
    /// string it is cut from, each walked as values of their own so a leaf
    /// shared by a rope doubled on itself is counted once. Only a string
    /// holding its own characters is counted by its length.
    fn string_held(&self, raw: qjs::JSValue, walk: &mut Walk<'js>) -> usize {
        let mut a = std::mem::MaybeUninit::<qjs::JSValue>::uninit();
        let mut b = std::mem::MaybeUninit::<qjs::JSValue>::uninit();
        // SAFETY: `raw` is a live string; what is returned is duplicated
        // for us, `a` when 1 or 2 is returned and `b` when 2 is.
        let parts = unsafe { JS_GetStringParts(raw, a.as_mut_ptr(), b.as_mut_ptr()) };
        if parts >= 1 {
            // SAFETY: filled in and owned, as above.
            walk.stack
                .push(unsafe { Value::from_raw(self.ctx().clone(), a.assume_init()) });
        }
        if parts == 2 {
            // SAFETY: as above.
            walk.stack
                .push(unsafe { Value::from_raw(self.ctx().clone(), b.assume_init()) });
        }
        if parts != 0 {
            return STRING;
        }
        let mut len = 0i64;
        // SAFETY: the context is entered and `raw` is a live string; its
        // length is read without making anything.
        if unsafe { qjs::JS_GetLength(self.ctx().as_raw().as_ptr(), raw, &mut len) } < 0 {
            let _ = self.ctx().catch();
        }
        STRING + usize::try_from(len).unwrap_or(0)
    }
}
