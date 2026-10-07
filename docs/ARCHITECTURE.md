# Riggerator architecture

## Crates

| Crate | Responsibility |
|---|---|
| `rg-mesh` | `Mesh` (positions + n-gon faces with face-varying UVs), primitives, modeling operations (`ops`), ear-clipping triangulation, render tessellation with split normals, ray casting. Pure data; no scene knowledge. |
| `rg-anim` | `AnimCurve` with Maya tangent types and infinity modes. Cubic Hermite evaluation; auto tangents use a Fritsch–Carlson limiter so curves never overshoot. |
| `rg-core` | The scene: `Scene` holds a DAG of `Node`s (transform + shape kind), materials, constraints, driven keys, display layers, time & render settings, selection and component selection. Also `History` (undo). |
| `rg-io` | Serialization: native `.rgs` (serde JSON), Maya ASCII subset, OBJ/MTL, glTF export. |
| `rg-render` | CPU path tracer used by the Render View, Hypershade swatches and `rg-batch`. |
| `rg-script` | RigScript: lexer/parser/interpreter for a MEL-compatible language and the Maya command set operating on a `Scene`. |
| `apps/riggerator` | egui UI + OpenGL viewport. |
| `apps/rg-batch` | Headless command-line runner. |

## Scene model

Maya separates transform and shape nodes; Riggerator merges them: each `Node` has a `Transform`
(translate / rotate (Euler degrees, rotate order) / scale / pivot) and a `NodeKind`
(`Transform`, `Mesh`, `Camera`, `Light`, `Joint`, `Locator`, `Curve`, `IkHandle`). Local matrices follow Maya:
`T · Tpivot · JointOrient · R · S · Tpivot⁻¹`, with Maya rotate-order semantics (`xyz` applies X first).

Attributes are addressed by Maya names (`translateX`/`tx`, `rotateY`, `visibility`, `focalLength`,
`blendShape.smile`, `bend1.curvature`, user attributes…). `Scene::get_attr`/`set_attr` are the single entry
points used by the channel box, MEL, animation and driven keys.

## Evaluation

Animation curves are applied by `Scene::set_time` — like Maya, curve output is written into the attribute
values. `Scene::evaluate()` then produces an `Evaluated` overlay without mutating the scene:

1. **Driven keys** (driver attribute value → curve → driven attribute)
2. **Hierarchy** world matrices
3. **Constraints** in creation order (parent, point, orient, scale, aim)
4. **IK handles** (analytic two-bone RP solver, FABRIK for longer chains, pole vectors, twist, blend), converted
   back to joint rotations
5. **Deformation**: blend shapes → nonlinear deformers → smooth skinning

The app caches the `Evaluated` result per scene revision (`Scene::revision` increases on every change).

## Undo

`History` stores scene snapshots. Meshes and skin clusters live behind `Arc`, so a snapshot copies the node table,
not the geometry. MEL run through `Interpreter::run` records one undo step per run; direct UI edits call
`App::checkpoint` first.

## Viewport

Each pane builds a `DrawList` on the UI thread (mesh draw items, grid/curve/joint line buffers, overlay
triangles) and hands it to an `egui_glow` paint callback. Tessellated meshes are cached on the GPU keyed by the
`Arc<Mesh>` pointer, and the cache entry keeps the `Arc` alive so addresses cannot be reused while cached.
Component points, manipulators, marquee, HUD and labels are painted with egui on top.

Picking uses CPU ray casts against evaluated meshes and screen-space distances to the line segments drawn for
joints, curves, cameras, lights and locators.

## Adding a command

1. Implement the operation in the relevant crate (`rg-mesh::ops`, `Scene`, …) with a unit test.
2. Add a branch to `rg-script/src/commands.rs` `dispatch` and a line to `COMMANDS` (help text). Declare
   zero-arity flags in `zero_arity` and multi-value flags in `arity`.
3. Expose it in a menu or shelf in `apps/riggerator/src/ui_top.rs` by calling `app.mel("yourCommand ...;")`
   so it echoes and undoes like everything else.
