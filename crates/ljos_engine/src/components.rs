//! The components loaded and their instances alive, for a tool that
//! shows what an application has loaded.
//!
//! Nothing is kept for this: it is one pass over the objects and the
//! loaded files, made when asked. Every instance made from a template is
//! kept by its root ([`Engine::keep_with`]), and the instance of each
//! component file an object of it is made from by that instance, so the
//! walk finds them all.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use ljos_ir::ComponentKind as Written;

use crate::engine::Engine;
use crate::ids::ObjectId;
use crate::incubator::Instance;

/// How a component is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ComponentKind {
    /// A file of its own.
    File,
    /// `component Name: Type { }` inside a file.
    Inline,
    /// A view's `delegate`.
    Delegate,
    /// The content of a `Component { }`, or another property a template
    /// is the value of.
    Template,
}

impl ComponentKind {
    pub fn name(self) -> &'static str {
        match self {
            ComponentKind::File => "file",
            ComponentKind::Inline => "inline",
            ComponentKind::Delegate => "delegate",
            ComponentKind::Template => "template",
        }
    }
}

/// One component of a loaded file, with its instances alive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadedComponent {
    /// The address of the file it is written in.
    pub url: String,
    /// Stable across edits that do not move it: the file's key,
    /// `file#Inline`, or for a template where it is written.
    pub id: String,
    /// The file's stem, the inline component's name, or for a template
    /// the type of its root.
    pub name: String,
    pub kind: ComponentKind,
    /// The root of each instance alive, in the order they were made.
    pub instances: Vec<ObjectId>,
    /// The objects of those instances still alive: an object of one is
    /// the root of a component it is made from too, and counts there as
    /// well.
    pub objects: usize,
    /// About the bytes those objects hold ([`Engine::object_bytes`]).
    pub bytes: usize,
}

impl Engine {
    /// Every component of every file loaded, with how many instances of
    /// it live and what they hold; a component nothing was made from is
    /// listed with none. By file, then as each file lists them.
    pub fn components(&self) -> Vec<LoadedComponent> {
        let store = self.inner.store.borrow();
        let mut stack: Vec<Rc<Instance>> = Vec::new();
        for header in store.objects.values() {
            let Some(ext) = header.ext.as_deref() else {
                continue;
            };
            for kept in &ext.keep {
                if let Ok(inst) = Rc::clone(kept).downcast::<Instance>() {
                    stack.push(inst);
                }
            }
        }
        // By file key and component.
        let mut found: HashMap<(String, u32), (Vec<ObjectId>, usize, usize)> = HashMap::new();
        let mut seen: HashSet<*const Instance> = HashSet::new();
        while let Some(inst) = stack.pop() {
            if !seen.insert(Rc::as_ptr(&inst)) {
                continue;
            }
            stack.extend(inst.inner().cloned());
            let Some(&root) = inst.objects.first() else {
                continue;
            };
            if !store.objects.contains_key(root) {
                continue;
            }
            let entry = found
                .entry((inst.file.key.clone(), inst.comp.0))
                .or_default();
            entry.0.push(root);
            for header in inst.objects.iter().filter_map(|o| store.objects.get(*o)) {
                entry.1 += 1;
                entry.2 += header.bytes();
            }
        }
        drop(store);
        let loader = self.inner.loader.borrow();
        let mut files: Vec<_> = loader.files.values().collect();
        files.sort_by(|a, b| a.url.cmp(&b.url).then_with(|| a.key.cmp(&b.key)));
        let mut out = Vec::new();
        for file in files {
            for (i, def) in file.ir.components.iter().enumerate() {
                let (mut instances, objects, bytes) = found
                    .remove(&(file.key.clone(), i as u32))
                    .unwrap_or_default();
                instances.sort();
                out.push(LoadedComponent {
                    url: file.url.to_string(),
                    id: def.id.clone(),
                    name: def.name.clone(),
                    kind: match def.kind {
                        Written::File => ComponentKind::File,
                        Written::Inline => ComponentKind::Inline,
                        Written::Template {
                            is_delegate: true, ..
                        } => ComponentKind::Delegate,
                        Written::Template { .. } => ComponentKind::Template,
                    },
                    instances,
                    objects,
                    bytes,
                });
            }
        }
        out
    }
}
