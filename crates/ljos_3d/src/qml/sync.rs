//! A view's scene kept and patched where its objects changed.
//!
//! [`build`](super::build) reads every object of a view each time; a
//! host that reads the scene every frame an object moves pays for all of
//! them to move one. Here the view is read in [`Piece`]s, one per object
//! and one each for the view's own members and its environment, and a
//! [`SceneSync`] keeps what each piece read and the scene they made. A
//! host tracks the reads of each piece apart, as its graph tracks a
//! binding, and on the next frame hands over the pieces whose reads
//! changed: a piece that changed its values patches its nodes in place,
//! and one that changed what the scene is made of (an object added,
//! removed or moved to another parent, a file loaded into more nodes)
//! puts the scene together again from the reads kept, reading only the
//! objects that are new. Either way the scene is the one
//! [`build`](super::build) would make.

use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::sync::Arc;

use super::build::{
    environment, given, grid_lines, kind, material_of, mesh_file, num, on, shadow, skeleton_joints,
    transform,
};
use super::{Built, Prop3d as P, SceneSource, Type3d as T, defaults as d};
use crate::geometry::CustomGeometry;
use crate::instancing::Skin;
use crate::math::Mat4;
use crate::scene::{Environment, MeshSource, Model, Node, NodeId, NodeKind, Scene};

/// A part of a view read on its own: what a host tracks apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Piece<O> {
    /// The view's own members: its nodes, its imported scene, its camera.
    View,
    /// The view's environment and the grids declared in it.
    Environment,
    /// One object found among the view's nodes or their children, with
    /// what its nodes are made of: its materials, textures, mesh and the
    /// objects it names. Not its children's own members.
    Node(O),
}

/// What reading one [`Piece`] found: [`read`].
#[derive(Clone, Debug, PartialEq)]
pub struct Read<O>(Found<O>);

#[derive(Clone, Debug, PartialEq)]
enum Found<O> {
    View {
        data: Vec<O>,
        import: Option<O>,
        camera: Option<O>,
    },
    Environment {
        /// Boxed: far larger than the other reads.
        environment: Box<Environment>,
        grids: Vec<Node>,
    },
    /// `None` for an object that is not a node, such as a material
    /// declared among the nodes.
    Node(Option<Box<Part<O>>>),
}

/// An object's nodes, as read, before the scene gives them their places.
#[derive(Clone, Debug, PartialEq)]
struct Part<O> {
    /// Its own node first, then what its mesh is made of and what its
    /// loader loaded, each with the index of its parent among these.
    nodes: Vec<(Option<usize>, Node)>,
    /// The ones that take the object's instance root and skin: its own,
    /// if it is a model, and its meshes' parts.
    later: Vec<usize>,
    /// Nodes a pick names by another object than this one's own node.
    loaded: Vec<(usize, O)>,
    /// For a loaded file, the node each of its nodes was in the scene it
    /// was loaded into: what its skins and instancing name.
    local: HashMap<NodeId, usize>,
    children: Vec<O>,
    quad: Option<(Node, O)>,
    light: bool,
    scope: Option<O>,
    instance_root: Option<O>,
    skin: Option<(Vec<Option<O>>, Vec<Mat4>)>,
}

impl<O: PartialEq> Part<O> {
    /// Whether `other` makes the same nodes in the same places, so that
    /// its values can be written over this one's.
    fn same_shape(&self, other: &Part<O>) -> bool {
        self.nodes.len() == other.nodes.len()
            && self.nodes.iter().zip(&other.nodes).all(|(a, b)| a.0 == b.0)
            && self.later == other.later
            && self.loaded == other.loaded
            && self.children == other.children
            && self.quad.as_ref().map(|q| &q.1) == other.quad.as_ref().map(|q| &q.1)
            && self.light == other.light
    }
}

/// Reads `piece` of the View3D `view` from `src`. A host that tracks reads
/// runs this inside the tracking of that piece alone.
pub fn read<S: SceneSource>(src: &S, view: S::Object, piece: Piece<S::Object>) -> Read<S::Object> {
    Read(match piece {
        Piece::View => Found::View {
            data: src.objects(view, P::Data),
            import: src.object(view, P::ImportScene),
            camera: src.object(view, P::Camera),
        },
        Piece::Environment => {
            let env_object = src.object(view, P::Environment);
            let mut grids = Vec::new();
            if let Some(env) = env_object.filter(|&env| src.is(env, T::SceneEnvironment)) {
                // Declared among the environment's data, drawn in the scene.
                for grid in src.objects(env, P::Data) {
                    if let Some(lines) = src.infinite_grid(grid) {
                        grids.extend(grid_lines(&lines));
                    }
                }
            }
            Found::Environment {
                environment: Box::new(
                    env_object.map_or_else(Environment::default, |env| environment(src, view, env)),
                ),
                grids,
            }
        }
        Piece::Node(obj) => Found::Node(part(src, view, obj).map(Box::new)),
    })
}

fn part<S: SceneSource>(s: &S, view: S::Object, obj: S::Object) -> Option<Part<S::Object>> {
    // Materials, environments and anything else declared among the nodes
    // are not drawn by themselves.
    if !s.is(obj, T::Node) {
        return None;
    }
    let mut n = Node::new(kind(s, view, obj)).with_transform(transform(s, obj));
    n.visible = on(s, obj, P::Visible, d::node::VISIBLE);
    n.opacity = num(s, obj, P::Opacity, d::node::OPACITY);
    let is_model = s.is(obj, T::Model);
    if is_model {
        n.casts_shadows = on(s, obj, P::CastsShadows, d::model::CASTS_SHADOWS);
        n.receives_shadows = on(s, obj, P::ReceivesShadows, d::model::RECEIVES_SHADOWS);
        n.instancing = s.object(obj, P::Instancing).and_then(|t| s.instancing(t));
        n.depth_bias = num(s, obj, P::DepthBias, d::model::DEPTH_BIAS);
        n.casts_reflections = on(s, obj, P::CastsReflections, true);
        n.receives_reflections = on(s, obj, P::ReceivesReflections, false);
    }
    n.shadow = shadow(s, obj);
    let light = s.is(obj, T::Light);
    let mut part = Part {
        nodes: vec![(None, n)],
        later: if is_model { vec![0] } else { Vec::new() },
        loaded: Vec::new(),
        local: HashMap::new(),
        children: Vec::new(),
        quad: None,
        light,
        scope: None,
        instance_root: None,
        skin: None,
    };
    if is_model && let Some(file) = mesh_file(s, view, obj) {
        // The file's sub-meshes, a scene node each, as a `RuntimeLoader`
        // adds a file's primitives: each is culled, sorted and drawn with
        // its own material, and the model's own settings hold for all of
        // them. The shared geometries are one upload.
        let weights: Vec<f32> = s
            .objects(obj, P::MorphTargets)
            .into_iter()
            .map(|t| num(s, t, P::Weight, d::morph_target::WEIGHT))
            .collect();
        let parts = file.parts.iter().enumerate();
        let parts = parts.filter(|(_, p)| !p.empty);
        add_parts(
            s,
            view,
            obj,
            &mut part,
            parts.map(|(i, _)| (i, file.morphed(i, &weights))),
        );
    }
    if is_model && let Some(g) = given(s, obj) {
        add_parts(
            s,
            view,
            obj,
            &mut part,
            g.parts().iter().cloned().enumerate(),
        );
    }
    // A loaded model's nodes are the loader's first children. Each mesh is
    // named by the Model the loader made for it.
    #[cfg(feature = "gltf")]
    if let Some(loaded) = s.loaded(obj) {
        add_loaded(s, obj, &mut part, &loaded);
    }
    part.children = s.objects(obj, P::Data);
    // Its items, as one picture; the pick finds them by their holder.
    part.quad = s.items_quad(view, obj);
    // After the walk: a scope may be declared after its light, and an
    // instance root or a joint after its model.
    if light {
        part.scope = s.object(obj, P::Scope);
    }
    if is_model {
        part.instance_root = s.object(obj, P::InstanceRoot);
        part.skin = skin_of(s, obj);
    }
    Some(part)
}

/// Adds what a model's mesh is made of under its own node, each with the
/// material of its place.
fn add_parts<S: SceneSource>(
    s: &S,
    view: S::Object,
    obj: S::Object,
    part: &mut Part<S::Object>,
    parts: impl Iterator<Item = (usize, Arc<CustomGeometry>)>,
) {
    let materials = s.objects(obj, P::Materials);
    let own = &part.nodes[0].1;
    let (casts, receives, instancing) = (
        own.casts_shadows,
        own.receives_shadows,
        own.instancing.clone(),
    );
    for (i, mesh) in parts {
        // "If the number of materials is less than the sub-meshes, the
        // last material in the list is used for subsequent sub-meshes."
        let m = materials.get(i).or(materials.last()).copied();
        let Some(material) = material_of(s, view, m) else {
            continue;
        };
        let mut n = Node::new(NodeKind::Model(Model {
            mesh: MeshSource::Custom(mesh),
            material,
        }));
        n.casts_shadows = casts;
        n.receives_shadows = receives;
        n.instancing.clone_from(&instancing);
        let at = part.nodes.len();
        part.nodes.push((Some(0), n));
        part.loaded.push((at, obj));
        part.later.push(at);
    }
}

/// Adds the loader `obj`'s file under its own node. The loader's table, if
/// it has one, copies the whole file: each mesh is copied in the loader's
/// space.
#[cfg(feature = "gltf")]
fn add_loaded<S: SceneSource>(
    s: &S,
    obj: S::Object,
    part: &mut Part<S::Object>,
    loaded: &super::Loaded<S::Object>,
) {
    // Loaded into a scene of its own, under a node that stands for the
    // loader's, and taken over in the order it was added.
    let mut file = Scene::new();
    let root = file.add(None, Node::new(NodeKind::Group));
    let placed = loaded.asset.add_to(&mut file, root);
    let instancing = s
        .object(obj, P::Instancing)
        .and_then(|t| s.instancing(t))
        .map(|i| crate::instancing::Instancing {
            root: Some(root),
            ..i
        });
    let first = part.nodes.len();
    part.local.insert(root, 0);
    for (k, (id, _)) in file.added().skip(1).enumerate() {
        part.local.insert(id, first + k);
    }
    for (_, node) in file.added().skip(1) {
        let parent = node.parent().and_then(|p| part.local.get(&p).copied());
        part.nodes.push((parent, node.clone()));
    }
    for (index, model) in placed.models {
        let Some(&at) = part.local.get(&model) else {
            continue;
        };
        if let Some(i) = &instancing {
            part.nodes[at].1.instancing = Some(i.clone());
        }
        if let Some((_, m)) = loaded.models.iter().find(|(n, _)| *n == index) {
            part.loaded.push((at, *m));
        }
    }
}

/// The joints a model's vertices follow, `skin`'s else `skeleton`'s, as
/// objects; the scene gives them their nodes.
#[allow(clippy::type_complexity)]
fn skin_of<S: SceneSource>(s: &S, obj: S::Object) -> Option<(Vec<Option<S::Object>>, Vec<Mat4>)> {
    if let Some(sk) = s.object(obj, P::Skin)
        && s.is(sk, T::Skin)
    {
        Some((
            s.object_slots(sk, P::Joints),
            s.matrices(sk, P::InverseBindPoses),
        ))
    } else if let Some(sk) = s.object(obj, P::Skeleton)
        && s.is(sk, T::Skeleton)
    {
        Some((skeleton_joints(s, sk), s.matrices(obj, P::InverseBindPoses)))
    } else {
        None
    }
}

/// Where an object's nodes are in the scene.
#[derive(Debug)]
struct Placed {
    nodes: Vec<NodeId>,
    quad: Option<NodeId>,
}

/// What [`SceneSync::sync`] did.
#[derive(Debug)]
pub struct Synced<O> {
    /// The scene now.
    pub scene: Arc<Scene>,
    /// Whether it draws differently from the one before.
    pub changed: bool,
    /// How many pieces were read for it.
    pub read: usize,
    /// The pieces no longer in the view, whose tracking the host can drop.
    pub gone: Vec<Piece<O>>,
}

/// A view's scene and what its pieces read. See the [module](self).
#[derive(Debug)]
pub struct SceneSync<O> {
    reads: HashMap<Piece<O>, Read<O>>,
    placed: HashMap<O, Placed>,
    /// The objects reached by the last walk, nodes or not.
    reached: HashSet<O>,
    grids: Vec<NodeId>,
    scene: Arc<Scene>,
    ids: HashMap<O, NodeId>,
    loaded: Vec<(NodeId, O)>,
    walked: bool,
}

impl<O> Default for SceneSync<O> {
    fn default() -> Self {
        SceneSync {
            reads: HashMap::new(),
            placed: HashMap::new(),
            reached: HashSet::new(),
            grids: Vec::new(),
            scene: Arc::default(),
            ids: HashMap::new(),
            loaded: Vec::new(),
            walked: false,
        }
    }
}

impl<O: Copy + Eq + Hash> SceneSync<O> {
    pub fn new() -> Self {
        SceneSync::default()
    }

    /// The scene, as of the last [`sync`](Self::sync).
    pub fn scene(&self) -> &Arc<Scene> {
        &self.scene
    }

    /// The node each object became: what a pick needs.
    pub fn ids(&self) -> &HashMap<O, NodeId> {
        &self.ids
    }

    /// The nodes that are part of an object without being its own node
    /// ([`Built::loaded`](super::Built::loaded)).
    pub fn loaded(&self) -> &[(NodeId, O)] {
        &self.loaded
    }

    /// What the walk made, as [`build`](super::build) gives it.
    pub(super) fn into_built(self) -> Built<O> {
        Built {
            scene: Arc::unwrap_or_clone(self.scene),
            ids: self.ids,
            loaded: self.loaded,
        }
    }

    /// What the scene has otherwise than the one [`build`](super::build)
    /// makes of `src` now, `None` when nothing: what a host checks a
    /// patched scene against while it is tested.
    pub fn drift<S: SceneSource<Object = O>>(&self, src: &S, view: O) -> Option<String> {
        self.scene.difference(&super::build(src, view).scene)
    }

    /// Brings the scene up to date with `changed`, the pieces whose reads
    /// changed since the last sync, reading each with `read`; a piece seen
    /// for the first time is read with it too. The first sync reads every
    /// piece and needs no `changed`.
    pub fn sync(
        &mut self,
        changed: impl IntoIterator<Item = Piece<O>>,
        mut read: impl FnMut(Piece<O>) -> Read<O>,
    ) -> Synced<O> {
        let before = self.scene.revision();
        let mut count = 0;
        let mut whole = !self.walked;
        let mut patch = Vec::new();
        let mut seen = HashSet::new();
        for piece in changed {
            if !self.walked || !seen.insert(piece) {
                continue;
            }
            // An object no longer in the view: nothing of it is drawn.
            if let Piece::Node(o) = piece
                && !self.reached.contains(&o)
            {
                continue;
            }
            let fresh = read(piece);
            count += 1;
            let old = self.reads.insert(piece, fresh);
            let new = &self.reads[&piece];
            if old.as_ref() == Some(new) {
                continue;
            }
            let same = match (old.as_ref().map(|r| &r.0), &new.0) {
                (
                    Some(Found::View { data, import, .. }),
                    Found::View {
                        data: d2,
                        import: i2,
                        ..
                    },
                ) => data == d2 && import == i2,
                (Some(Found::Environment { grids, .. }), Found::Environment { grids: g2, .. }) => {
                    grids.len() == g2.len()
                }
                (Some(Found::Node(Some(a))), Found::Node(Some(b))) => {
                    a.same_shape(b) && self.placed.contains_key(&node_of(piece))
                }
                // Not a node then or now: it is not drawn either way.
                (Some(Found::Node(None)), Found::Node(None)) => continue,
                _ => false,
            };
            if same {
                patch.push(piece);
            } else {
                whole = true;
            }
        }
        let mut gone = Vec::new();
        if whole {
            gone = self.walk(&mut read, &mut count);
        } else if !patch.is_empty() {
            self.patch(&patch);
        }
        Synced {
            scene: self.scene.clone(),
            changed: self.scene.revision() != before,
            read: count,
            gone,
        }
    }

    /// The read of `piece`, read now if it was not before.
    fn got(
        &mut self,
        piece: Piece<O>,
        read: &mut impl FnMut(Piece<O>) -> Read<O>,
        count: &mut usize,
    ) -> &Found<O> {
        &self
            .reads
            .entry(piece)
            .or_insert_with(|| {
                *count += 1;
                read(piece)
            })
            .0
    }

    /// Writes the values of `pieces`, each of the same shape as before,
    /// over their nodes.
    fn patch(&mut self, pieces: &[Piece<O>]) {
        let mut nodes: Vec<(NodeId, Node)> = Vec::new();
        let mut environment = None;
        let mut camera = None;
        for &piece in pieces {
            match &self.reads[&piece].0 {
                Found::View { camera: c, .. } => {
                    camera = Some(c.and_then(|c| self.ids.get(&c).copied()));
                }
                Found::Environment {
                    environment: env,
                    grids,
                } => {
                    environment = Some(&**env);
                    nodes.extend(self.grids.iter().copied().zip(grids.iter().cloned()));
                }
                Found::Node(Some(part)) => {
                    let Some(placed) = self.placed.get(&node_of(piece)) else {
                        continue;
                    };
                    for (i, &id) in placed.nodes.iter().enumerate() {
                        let node = finished(part, i, &placed.nodes, &self.ids)
                            .unwrap_or_else(|| part.nodes[i].1.clone());
                        nodes.push((id, node));
                    }
                    if let (Some(id), Some((quad, _))) = (placed.quad, &part.quad) {
                        nodes.push((id, quad.clone()));
                    }
                }
                Found::Node(None) => {}
            }
        }
        nodes.retain_mut(|(id, node)| self.scene.would_change(*id, node));
        let env_changes = environment.is_some_and(|e| e != self.scene.environment());
        let camera_changes = camera.is_some_and(|c| c != self.scene.camera());
        if nodes.is_empty() && !env_changes && !camera_changes {
            return;
        }
        let scene = Arc::make_mut(&mut self.scene);
        for (id, node) in nodes {
            scene.set_node(id, node);
        }
        if let Some(env) = environment {
            scene.set_environment(env.clone());
        }
        if let Some(c) = camera {
            scene.set_camera(c);
        }
    }

    /// Puts the scene together again from the reads, in the order
    /// [`build`](super::build) walks the view; returns the pieces no
    /// longer reached.
    fn walk(
        &mut self,
        read: &mut impl FnMut(Piece<O>) -> Read<O>,
        count: &mut usize,
    ) -> Vec<Piece<O>> {
        self.walked = true;
        let mut scene = Scene::new();
        self.placed.clear();
        self.reached.clear();
        self.ids.clear();
        self.loaded.clear();
        self.grids.clear();
        let (data, import, camera) = match self.got(Piece::View, read, count) {
            Found::View {
                data,
                import,
                camera,
            } => (data.clone(), *import, *camera),
            _ => (Vec::new(), None, None),
        };
        enum Step<O> {
            Visit(Option<NodeId>, O),
            Quad(NodeId, O),
        }
        let mut stack: Vec<Step<O>> = import.into_iter().map(|o| Step::Visit(None, o)).collect();
        stack.extend(data.into_iter().rev().map(|o| Step::Visit(None, o)));
        let mut finish = Vec::new();
        while let Some(step) = stack.pop() {
            let (parent, obj) = match step {
                Step::Visit(parent, obj) => (parent, obj),
                Step::Quad(own, obj) => {
                    if let Some(Found::Node(Some(part))) =
                        self.reads.get(&Piece::Node(obj)).map(|r| &r.0)
                        && let Some((quad, holder)) = &part.quad
                    {
                        let q = scene.add(Some(own), quad.clone());
                        self.ids.insert(*holder, q);
                        if let Some(placed) = self.placed.get_mut(&obj) {
                            placed.quad = Some(q);
                        }
                    }
                    continue;
                }
            };
            self.reached.insert(obj);
            if self.ids.contains_key(&obj) {
                continue;
            }
            self.got(Piece::Node(obj), read, count);
            let Some(Found::Node(Some(part))) = self.reads.get(&Piece::Node(obj)).map(|r| &r.0)
            else {
                continue;
            };
            let mut ids = Vec::with_capacity(part.nodes.len());
            for (at, node) in &part.nodes {
                let parent = match at {
                    None => parent,
                    Some(at) => Some(ids[*at]),
                };
                ids.push(scene.add(parent, node.clone()));
            }
            let own = ids[0];
            self.ids.insert(obj, own);
            for &(at, o) in &part.loaded {
                self.loaded.push((ids[at], o));
            }
            if !part.later.is_empty() || part.light || !part.local.is_empty() {
                finish.push(obj);
            }
            stack.push(Step::Quad(own, obj));
            stack.extend(
                part.children
                    .iter()
                    .rev()
                    .map(|&c| Step::Visit(Some(own), c)),
            );
            self.placed.insert(
                obj,
                Placed {
                    nodes: ids,
                    quad: None,
                },
            );
        }
        // Once every node has its place: what a node names may come after it.
        for obj in finish {
            let (Some(Found::Node(Some(part))), Some(placed)) = (
                self.reads.get(&Piece::Node(obj)).map(|r| &r.0),
                self.placed.get(&obj),
            ) else {
                continue;
            };
            for (i, &id) in placed.nodes.iter().enumerate() {
                if let Some(node) = finished(part, i, &placed.nodes, &self.ids) {
                    scene.set_node(id, node);
                }
            }
        }
        if let Found::Environment { environment, grids } =
            self.got(Piece::Environment, read, count).clone()
        {
            for grid in grids {
                self.grids.push(scene.add(None, grid));
            }
            scene.set_environment(*environment);
        }
        // A camera that is not in the scene leaves the choice to the scene:
        // its first camera.
        scene.set_camera(camera.and_then(|c| self.ids.get(&c).copied()));
        let reached = &self.reached;
        let mut gone = Vec::new();
        self.reads.retain(|piece, _| match piece {
            Piece::Node(o) if !reached.contains(o) => {
                gone.push(*piece);
                false
            }
            _ => true,
        });
        self.scene = Arc::new(scene);
        gone
    }
}

fn node_of<O: Copy>(piece: Piece<O>) -> O {
    match piece {
        Piece::Node(o) => o,
        // Only an object's piece is placed.
        _ => unreachable!("only an object's piece has nodes"),
    }
}

/// The node `i` of `part` with what it names given its nodes: a light's
/// scope, a model's instance root and joints, a loaded file's own; `None`
/// when it names nothing.
fn finished<O: Copy + Eq + Hash>(
    part: &Part<O>,
    i: usize,
    placed: &[NodeId],
    ids: &HashMap<O, NodeId>,
) -> Option<Node> {
    let later = part.later.contains(&i);
    let scoped = i == 0 && part.light;
    let base = &part.nodes[i].1;
    let names = base.instancing.as_ref().is_some_and(|t| t.root.is_some())
        || base.skin.is_some()
        || base.scope.is_some();
    let local = !part.local.is_empty() && i > 0 && !later && names;
    if !later && !scoped && !local {
        return None;
    }
    let mut node = base.clone();
    if local {
        let at = |id: NodeId| part.local.get(&id).map(|&k| placed[k]);
        if let Some(t) = node.instancing.as_mut() {
            t.root = t.root.and_then(at);
        }
        if let Some(skin) = node.skin.as_mut() {
            // A joint the file leaves out stays where the loader is.
            for j in &mut skin.joints {
                *j = at(*j).unwrap_or(placed[0]);
            }
        }
        node.scope = node.scope.and_then(at);
    }
    if scoped {
        node.scope = part.scope.and_then(|o| ids.get(&o).copied());
    }
    if later {
        let root = part.instance_root.and_then(|r| ids.get(&r).copied());
        if let Some(t) = node.instancing.as_mut() {
            t.root = root;
        }
        node.skin = part.skin.as_ref().and_then(|(joints, inverse_bind)| {
            let joints = joints
                .iter()
                .map(|j| ids.get(&(*j)?).copied())
                .collect::<Option<Vec<_>>>()?;
            Some(Skin {
                joints,
                inverse_bind: inverse_bind.clone(),
            })
        });
    }
    Some(node)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::Vec3;
    use crate::qml::{MapGraph, MapValue, build};

    /// A View3D with a camera, a light, and a parent node holding two
    /// cubes, each with its own DefaultMaterial.
    struct Fixture {
        g: MapGraph,
        view: usize,
        camera: usize,
        light: usize,
        parent: usize,
        cubes: [usize; 2],
        materials: [usize; 2],
    }

    fn fixture() -> Fixture {
        let mut g = MapGraph::new();
        let view = g.add(T::View3D);
        let camera = g.add(T::PerspectiveCamera);
        g.set(camera, "z", MapValue::Number(300.0));
        let light = g.add(T::DirectionalLight);
        let parent = g.add(T::Node);
        let mut cubes = [0; 2];
        let mut materials = [0; 2];
        for i in 0..2 {
            let cube = g.add(T::Model);
            g.set(cube, "source", MapValue::Url("#Cube".into()));
            g.set(cube, "x", MapValue::Number(i as f64 * 50.0));
            let material = g.add(T::DefaultMaterial);
            g.set(cube, "materials", MapValue::Objects(vec![material]));
            g.append(parent, cube);
            cubes[i] = cube;
            materials[i] = material;
        }
        g.append(view, camera)
            .append(view, light)
            .append(view, parent);
        Fixture {
            g,
            view,
            camera,
            light,
            parent,
            cubes,
            materials,
        }
    }

    fn first(f: &Fixture) -> SceneSync<usize> {
        let mut kept = SceneSync::new();
        let synced = kept.sync([], |p| read(&f.g, f.view, p));
        assert!(synced.read > 0);
        kept
    }

    /// Syncs `changed` and checks the scene is the one a build makes now,
    /// with the same nodes for the same objects.
    fn synced(f: &Fixture, kept: &mut SceneSync<usize>, changed: &[Piece<usize>]) -> Synced<usize> {
        let synced = kept.sync(changed.iter().copied(), |p| read(&f.g, f.view, p));
        let built = build(&f.g, f.view);
        assert!(
            built.scene.same_as(kept.scene()),
            "patched {:#?}\nbuilt {:#?}",
            kept.scene(),
            built.scene
        );
        assert_eq!(&built.ids, kept.ids());
        assert_eq!(built.loaded.as_slice(), kept.loaded());
        assert_eq!(kept.drift(&f.g, f.view), None);
        synced
    }

    #[test]
    fn a_first_sync_is_the_build() {
        let f = fixture();
        let kept = first(&f);
        assert!(build(&f.g, f.view).scene.same_as(kept.scene()));
    }

    #[test]
    fn a_moved_node_is_read_alone_and_patched() {
        let mut f = fixture();
        let mut kept = first(&f);
        let before = kept.scene().clone();
        f.g.set(f.cubes[1], "y", MapValue::Number(25.0));
        let s = synced(&f, &mut kept, &[Piece::Node(f.cubes[1])]);
        assert_eq!(s.read, 1);
        assert!(s.changed);
        assert!(s.gone.is_empty());
        // Patched in place: every other node is the one it was.
        let id = kept.ids()[&f.cubes[0]];
        assert!(std::ptr::eq(
            before.node(id).unwrap(),
            kept.scene().node(id).unwrap()
        ));
    }

    #[test]
    fn a_piece_read_the_same_changes_nothing() {
        let f = fixture();
        let mut kept = first(&f);
        let before = kept.scene().clone();
        let s = synced(&f, &mut kept, &[Piece::Node(f.cubes[0]), Piece::View]);
        assert!(!s.changed);
        assert!(Arc::ptr_eq(&before, kept.scene()));
    }

    #[test]
    fn a_material_change_patches_its_model() {
        let mut f = fixture();
        let mut kept = first(&f);
        f.g.set(
            f.materials[0],
            "diffuseColor",
            MapValue::Color([1.0, 0.0, 0.0, 1.0]),
        );
        // The material is read with the model that names it.
        let s = synced(&f, &mut kept, &[Piece::Node(f.cubes[0])]);
        assert!(s.changed);
    }

    #[test]
    fn a_light_change_patches_the_light() {
        let mut f = fixture();
        let mut kept = first(&f);
        f.g.set(f.light, "brightness", MapValue::Number(3.0));
        assert!(synced(&f, &mut kept, &[Piece::Node(f.light)]).changed);
        // A scope names another node: given after every node has its place.
        f.g.set(f.light, "scope", MapValue::Object(f.parent));
        assert!(synced(&f, &mut kept, &[Piece::Node(f.light)]).changed);
    }

    #[test]
    fn a_camera_moved_or_chosen_patches_the_scene() {
        let mut f = fixture();
        let mut kept = first(&f);
        f.g.set(f.camera, "z", MapValue::Number(500.0));
        assert!(synced(&f, &mut kept, &[Piece::Node(f.camera)]).changed);
        f.g.set(f.view, "camera", MapValue::Object(f.camera));
        let s = synced(&f, &mut kept, &[Piece::View]);
        assert_eq!(s.read, 1);
    }

    #[test]
    fn a_hidden_node_is_patched() {
        let mut f = fixture();
        let mut kept = first(&f);
        f.g.set(f.parent, "visible", MapValue::Bool(false));
        assert!(synced(&f, &mut kept, &[Piece::Node(f.parent)]).changed);
        f.g.set(f.parent, "visible", MapValue::Bool(true));
        assert!(synced(&f, &mut kept, &[Piece::Node(f.parent)]).changed);
    }

    #[test]
    fn an_added_node_reads_only_itself() {
        let mut f = fixture();
        let mut kept = first(&f);
        let cube = f.g.add(T::Model);
        f.g.set(cube, "source", MapValue::Url("#Sphere".into()));
        f.g.append(f.parent, cube);
        let s = synced(&f, &mut kept, &[Piece::Node(f.parent)]);
        // The parent, whose children changed, and the one it gained.
        assert_eq!(s.read, 2);
        assert!(s.changed);
    }

    #[test]
    fn a_removed_node_is_gone_with_its_piece() {
        let mut f = fixture();
        let mut kept = first(&f);
        f.g.set(f.parent, "data", MapValue::Objects(vec![f.cubes[1]]));
        let s = synced(&f, &mut kept, &[Piece::Node(f.parent)]);
        assert_eq!(s.gone, vec![Piece::Node(f.cubes[0])]);
        // What the gone one reads no longer reaches the scene.
        f.g.set(f.cubes[0], "x", MapValue::Number(-90.0));
        let s = synced(&f, &mut kept, &[Piece::Node(f.cubes[0])]);
        assert_eq!(s.read, 0);
        assert!(!s.changed);
    }

    #[test]
    fn a_reparented_node_keeps_its_read() {
        let mut f = fixture();
        let mut kept = first(&f);
        f.g.set(f.parent, "data", MapValue::Objects(vec![f.cubes[0]]));
        f.g.append(f.view, f.cubes[1]);
        let s = synced(&f, &mut kept, &[Piece::Node(f.parent), Piece::View]);
        assert_eq!(s.read, 2);
        assert!(s.gone.is_empty());
    }

    #[test]
    fn an_environment_change_patches_the_environment() {
        let mut f = fixture();
        let mut kept = first(&f);
        let env = f.g.add(T::SceneEnvironment);
        f.g.set(f.view, "environment", MapValue::Object(env));
        synced(&f, &mut kept, &[Piece::Environment]);
        f.g.set(env, "clearColor", MapValue::Color([0.0, 0.0, 1.0, 1.0]));
        f.g.set(env, "backgroundMode", MapValue::Number(1.0));
        let s = synced(&f, &mut kept, &[Piece::Environment]);
        assert_eq!(s.read, 1);
        assert!(s.changed);
    }

    #[test]
    fn many_changes_in_one_sync_end_where_a_build_does() {
        let mut f = fixture();
        let mut kept = first(&f);
        f.g.set(
            f.cubes[0],
            "position",
            MapValue::Vector3d(Vec3::new(1.0, 2.0, 3.0)),
        );
        f.g.set(f.light, "brightness", MapValue::Number(0.5));
        let extra = f.g.add(T::Node);
        f.g.append(f.cubes[1], extra);
        synced(
            &f,
            &mut kept,
            &[
                Piece::Node(f.cubes[0]),
                Piece::Node(f.light),
                Piece::Node(f.cubes[1]),
                Piece::Node(f.cubes[0]),
            ],
        );
    }
}
