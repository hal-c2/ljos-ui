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
    /// The walk stopped at its budget, and what was left was not counted.
    pub truncated: bool,
}

// About what QuickJS keeps for each: an object's header with its share of
// a shape, a property or array slot, a string's header.
const OBJECT: usize = 64;
const SLOT: usize = 16;
const STRING: usize = 16;

impl<'js> Scope<'js> {
    /// About the bytes the values of each group hold in the script heap:
    /// what each reaches through the own properties of ordinary objects
    /// and arrays, counted once. A getter is counted as a function, never
    /// called, so the walk runs no script. A function holds what its
    /// closure holds, which QuickJS does not show: only the function is
    /// counted, and so for a proxy, a host's object and the other kinds
    /// whose insides are the engine's. A reference no longer held is left
    /// out. Stops after `budget` values.
    pub fn held_by<'r, G>(&self, groups: impl IntoIterator<Item = G>, budget: usize) -> Held
    where
        G: IntoIterator<Item = &'r JsRef>,
    {
        let mut held = Held::default();
        let mut seen = HashSet::new();
        let mut stack = Vec::new();
        for group in groups {
            let mut bytes = 0;
            for r in group {
                if held.values >= budget {
                    held.truncated = true;
                    break;
                }
                // A value not made yet (an array a host handed over that no
                // script has read) is not in the heap: making it to look
                // would run the script's hook and grow what is measured.
                if r.key().is_none() {
                    continue;
                }
                match self.restore(r) {
                    Ok(v) => stack.push(v),
                    Err(_) => {
                        let _ = self.ctx().catch();
                    }
                }
                while let Some(v) = stack.pop() {
                    if held.values >= budget {
                        held.truncated = true;
                        stack.clear();
                        break;
                    }
                    let room = budget - held.values;
                    let (b, cut) = self.held_at(&v, room, &mut seen, &mut stack, &mut held.values);
                    bytes += b;
                    held.truncated |= cut;
                }
            }
            held.bytes.push(bytes);
        }
        held
    }

    /// What `v` itself holds, its first time; what it refers to goes on
    /// `stack`, at most `room` of its properties in all, and true when it
    /// had more.
    fn held_at(
        &self,
        v: &Value<'js>,
        room: usize,
        seen: &mut HashSet<usize>,
        stack: &mut Vec<Value<'js>>,
        values: &mut usize,
    ) -> (usize, bool) {
        let raw = v.as_raw();
        let ctx = self.ctx().as_raw().as_ptr();
        if !(v.is_object() || v.is_string()) {
            // Numbers, booleans and the like live in the slot that holds them.
            return (0, false);
        }
        // SAFETY: tagged as an object or a string, the payload is the
        // pointer to it, only compared here.
        let at = unsafe { qjs::JS_VALUE_GET_PTR(raw) } as usize;
        if !seen.insert(at) {
            return (0, false);
        }
        *values += 1;
        if v.is_string() {
            let mut len = 0i64;
            // SAFETY: the context is entered and `raw` is a live string; its
            // length is read without making anything.
            if unsafe { qjs::JS_GetLength(ctx, raw, &mut len) } < 0 {
                let _ = self.ctx().catch();
            }
            return (STRING + usize::try_from(len).unwrap_or(0), false);
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
            return (OBJECT + size as usize, false);
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
            return (OBJECT, false);
        }
        let len = names.len as usize;
        // Each property read is counted against the budget, a number's too:
        // an array of a million numbers is a million reads.
        let cut = len > room;
        for i in 0..len.min(room) {
            // SAFETY: `i` is within the table the engine filled in.
            let atom = unsafe { (*names.tab.add(i)).atom };
            let mut d = std::mem::MaybeUninit::<qjs::JSPropertyDescriptor>::uninit();
            // SAFETY: as above. A descriptor found holds owned references
            // to its value, getter and setter, each taken over by a Value.
            unsafe {
                match qjs::JS_GetOwnProperty(ctx, d.as_mut_ptr(), raw, atom) {
                    1 => {
                        let d = d.assume_init();
                        for part in [d.value, d.getter, d.setter] {
                            let part = Value::from_raw(self.ctx().clone(), part);
                            // Only what can hold more than its slot is walked.
                            if part.is_object() || part.is_string() {
                                stack.push(part);
                            }
                        }
                    }
                    0 => {}
                    _ => {
                        let _ = self.ctx().catch();
                    }
                }
            }
        }
        (OBJECT + len * SLOT, cut)
    }
}
