# LeopardCraft — next improvements

To-do list for the five agreed workstreams. Boxes stay unchecked until an item's
full scope lands. Each item names the code that has to change so the list can be
worked top to bottom without re-deriving the plan.

**Progress:** wave 1 implemented per-key F-curve selection, frame-snapped
dragging, deletion with a single undo step, and per-key interpolation in the
editor (Animation Core → Editor). Wave 2 added the channel registry
(`AnimChannel` with object/tag/material scope and component counts), a
dedicated four-component quaternion rotation track with shortest-path slerp,
STEP and normalised cubic sampling, registry-driven transform recording,
quaternion key move/delete/interpolation in the F-curve editor, plus evaluated
Copy Location weight and material roughness/metallic channels.

Wave 3 made the registry the editor's row source for every scope. The F-curve
editor can target the primary object, an individual Copy Location tag or the
selected material; its rows, labels and component counts come from
`AnimChannel`, and empty channels are listed so they can be keyed. `Key Channel`
records the selected row at the playhead. Tag-scope keys live in per-tag clips
(`Node::tag_animations`, with a dedicated undo command) and material keys in
`Material::animation`; both support per-key select, drag, interpolation and
delete as one undo step. Material recording reads base graph values so a new key
is not overwritten by the previous animated value.

Wave 4 added multi-key editing. Shift/command-click and marquee drags select
scalar and quaternion keys; dragging any selected key moves the whole selection
and preserves spacing. Copy/paste uses registry channels, so keys move between
objects, tags or materials with time offsets preserved from the earliest copied
key. Paste starts at the playhead, overwrites colliding keys, and is one undo
step alongside box-selected drag and multi-delete. The pre-change failure runs
for tag/material rows and the new box/copy tests were captured as runtime
assertions, not compile errors.

Wave 5 closed the glTF export loop. Morph weights now have their own clip
tracks, are sampled by the geometry evaluator, survive native format
round-trips, and are exported with node TRS animation as one seconds-based
`Take`. The GLB/glTF exporter writes bind-pose skins with inverse-bind matrices
and normalized four-influence attributes, named morph POSITION/NORMAL deltas,
and STEP/LINEAR/CUBICSPLINE samplers. Export parse tests cover skins, morphs,
animation channels and accessor bounds; glTF import remains open, while the
native F-curve editor now lists morph-weight rows.

Wave 6 added a four-influence weight-paint workflow. A bound modifier-free
editable mesh can be painted from the viewport with Add, Subtract, Replace and
Smooth brushes, radius, strength and constant/linear/smooth falloff. The
Skinning panel selects the active joint and the viewport shows its heat map;
each stamp compacts duplicate/zero slots, keeps the strongest four influences
and renormalizes. A whole drag is one undo step, and native-format loading now
repairs skin lengths, indices, duplicate slots and weight ranges. Wave 6 still
targeted all joints; Wave 7 completes the binding workflow.

Wave 7 completed the binding workflow. The Skinning panel now offers a chosen
joint checklist with All/None, deterministic proximity auto-weights with
Dropoff, Strength and Max influences controls, and weight-preserving rebind.
With automatic weights off, uncovered vertices stay unweighted and evaluate at
rest instead of falling back to the root joint. Brush strokes, heat maps,
per-stroke undo and normalization still apply after a rebind.

Wave 8 added inverse kinematics on the joint hierarchy. A tip Joint can carry
an `IkChain` with a target, optional pole, movable-ancestor count, iteration and
tolerance limits, and CCD damping. Exactly two movable joints use a closed-form
solve; one-joint and longer chains use damped CCD. Evaluation applies the solved
local rotations before skinning, unreachable targets clamp to the chain's reach, and
`Bake IK take` writes rotation keys for every frame in the playback range in one
undo step. Viewport joint picking, spline IK and the broader constraint system
remain separate work; Wave 9 adds per-joint rotation limits.

Wave 9 added clean-room per-joint rotation limits. A Joint can carry optional
local XYZ Euler minimum/maximum angles in degrees. The limits are normalized,
serialized with the joint and included in property undo; evaluation clamps
direct and animated joint rotation before solving, while the analytic two-bone
and damped CCD paths clamp after every joint correction. Zero-width ranges lock
an axis, reversed finite bounds are swapped, and non-finite values fall back to
the full range. The Joint inspector can add, remove, enable and edit the limits.
This is an approximation of Cheetah3D's IK Limits/IK Constraint tag, not a copy
of its heading/pitch/bank or documented 3D-only semantics.

Wave 10 added UV seam authoring. A mesh stores seams as canonical point pairs,
independent of creases. Edge-mode **Toggle Seam** in the Mesh Editing panel and
`Selection -> Toggle Seam` mark or unmark the selected edges as one undo step,
the 3D viewport draws seam edges green above crease blue, and the UV view draws
them over the UV wire and reports the count. Automatic unwrap now treats a seam
as a hard chart border instead of relying only on the normal-angle threshold,
subdivision inherits seams onto the split child edges, and seams survive point
compaction, welding, merging and native save/load. Seam authoring from the UV
view, select-all/clear commands and pinned UVs remain open.

Wave 11 added pinned UVs, the other half of the documented unwrap workflow. A
mesh stores pinned UV slots as sorted slot indices beside its seam and crease
lists, so pins persist through native save/load, point compaction, welding and
mesh merging and take part in the semantic signature. `Toggle Pinned UV` (UV
panel toolbar and `Selection -> Toggle Pinned UV`) pins or unpins the UV point
selected in the UV view as one undo step and stays disabled until a UV point is
selected. Pinned points draw yellow and the UV view reports the pinned count.
Unwrap, relax and normalize treat pins as hard constraints: a pinned slot keeps
its exact authored coordinate while the rest re-solves, and each pinned chart is
best-fit translated and uniformly scaled onto its pins before the pinned values
are written back exactly. Projections re-author the whole UV set and clear stale
pins; subdivision clears them too because it rebuilds per-point UVs.

Wave 12 added the **Fill Hole** polygon tool — chosen as the highest-impact open
item that fits one wave: the rigging, glTF-import and scripting gaps each need a
multi-wave data-model or API change, while Fill Hole is documented (sources 1
and 38), used daily, and closes a modelling-tool gap end to end. The manual says
the tool works "in point, edge and polygon mode", "can only be applied on raw
polygon objects", and that after clicking "one point on hole border ... Cheetah3D
will now automatically detect the rest of the hole border". LeopardCraft's
`lc_geom::open_holes` maps a component selection to the boundary loops it
touches, and `lc_geom::fill_holes` closes each loop with one n-gon reversed so it
winds with its neighbours. Mesh Editing's **Fill Hole** button and
`Tools -> Fill Hole` share one predicate: every selected editable mesh needs a
selected point on an open border, in any of the three component modes. Each fill
is one undo step, clears the component selection because indices change, reports
the number of open holes under the selection, preserves creases, seams and pins,
and round-trips through `.lcscene`.

Wave 13 added the **Split** polygon tool (sources 39 and 40) — chosen because the
higher-ranked gaps each still need multi-wave work: rigging and weight
acquisition need viewport joint picking plus a solver, glTF import needs a whole
second importer path, and the UV gap is a suite of chart tools. Split is a
documented single-page tool with one exact workflow and it is a daily modelling
operation, so it could be closed end to end. The manual says the tool "creates a
completely new polygon object with the content of the current polygon selection",
"can only be applied to raw polygon objects", "is only available in polygon mode",
and that the new object carries "all tags and selections"; a forum thread
(source 40) confirms the source keeps its polygons (`"you need to go back and
delete the bit you split off manually"`). LeopardCraft's `lc_geom::extract_faces`
copies the selected polygons with only the points, UV slots, creases, seams and
pins they use, and `AddNodesCmd` inserts every copy beside its source in one undo
step, carrying the source transform, flags, material slots, tags and per-tag
clips. Gating is polygon mode on editable meshes, the panel reports the polygon
count, and `Tools -> Split` mirrors the Mesh Editing button.

Wave 14 added the **Shell** polygon tool (source 41) — the largest remaining
modelling tool after Fill Hole and Split. The manual calls it "the tool of choice
if you want to make walls for a house or if you want to create a solid object out
of a plain surface", restricts it to "raw polygon objects" and polygon mode, and
documents three properties: **Inside** ("moved along the normal direction
(outside) or against the normal direction (inside)"), **Angle** ("which polygon
neighbors of a polygon selection belong to a shell") and **Quantize** ("the
distance you want to quantize the shell to"). Higher-ranked gaps were deferred
for the same reasons as wave 13: rigging needs viewport joint picking plus a
weighting solver, glTF import needs a second importer path, and the UV gap is a
suite of chart tools. `lc_geom::shell_region` implements the Angle grouping,
`lc_geom::shell_faces` offsets the region along averaged normals, appends the
reversed copy and the rim quads and applies the quantized distance, and Mesh
Editing's **Shell** row (thickness, Inside, Angle, Quantize) plus
`Tools -> Shell` share one predicate: polygon mode on editable meshes with a
selected polygon and a non-zero distance. The panel reports the region size, one
undo step covers the shell, and a shelled mesh round-trips with creases, seams,
pins, split parts and a filled hole intact.

Wave 15 closed a silently-ignored input in the existing **Relax** UV tool, taken
from this file's own gap list. `relax_uvs` averaged UVs per mesh point across the
whole mesh, so an authored seam — a documented chart cut (source 37: "A seam is
an edge where the mesh will tear up when calling the Unwrap UV command") — was
ignored and both sides of the cut collapsed onto one shared value. The manual
documents no Relax tool, so honouring seams there is a LeopardCraft clean-room
choice and is recorded as such. Relaxation now builds charts by flooding faces
across non-seam edges, relaxes per corner with pins as hard per-corner
constraints, holds seam and mesh-boundary corners when the pin-boundary option
is on, and keeps the no-pin re-fit into 0..1. `uv::tests::
relax_keeps_seams_as_chart_borders` parks two charts apart in UV space and
asserts the gap survives a relax with free borders and that a seam endpoint is
held with borders pinned; the old per-point code wrote the same averaged value
to corners on both sides, so that gap went to zero. Remaining: the splitter uses
seams only, not the unwrap's normal-angle charts, and there is no re-pack after
smoothing.

Wave 16 took the smallest coherent slice of the top-ranked interchange gap:
**glTF morph import**. The importer accepted `targets` and `weights` in a file
but read neither, so authored morph poses disappeared without a word. It now
builds a `MorphSet` from each primitive's POSITION target offsets (base = the
imported rest mesh), names targets from `mesh.extras.targetNames` or a target's
`extras.name`, and applies `mesh.weights` defaults with node-level `weights`
overriding them, so evaluation poses the imported mesh the way the file
describes. Data it cannot merge is refused with a message naming the mesh or
node — mixed primitive target counts, a name list of the wrong length, a target
accessor whose element count does not match the primitive, and a weights array
that does not match the target count — and a target with no POSITION accessor
imports as zero offsets rather than being dropped. Clamping imported weights to
0–1 follows LeopardCraft's documented clean-room range. `gltf::
animation_export_tests::imports_morph_targets_names_and_weights` round-trips a
two-target morph box through GLB and checks names, weights, delta offsets and
the posed vertex; `morph_import_refuses_mismatched_weight_counts` covers the
refusal. The old importer produced a plain mesh with no morph set, so both
fail. Remaining: skins, animation channels, CUBICSPLINE tangents and clip
duration → fps on import.

Wave 17 closed the remaining silence in the **glTF importer**: a file carrying
skins, animation clips, cameras or lights imported as bare geometry with those
authored features dropped and no word to the user — the same honesty failure as
the Shell clamp and seam fixes. The importer now returns an `ImportReport`
counting skinned mesh nodes, `animations` entries, camera nodes and
`KHR_lights_punctual` light nodes reached by the imported hierarchy, and
`ImportReport::summary()` turns that into one actionable sentence: what was
skipped, that glTF import reads meshes/materials/morph targets only, and that
the user can re-export without the data or recreate it in LeopardCraft. The
editor appends it to the Open/Import status line, the MCP `open_scene` payload
carries it as `import_report`, and headless or MCP startup prints it to stderr.
The import itself is unchanged: geometry, materials and morph targets still
load, and a mesh-only file produces an empty report and the same status as
before. `gltf::animation_export_tests::skinned_animated_and_camera_data_is_reported_as_skipped`
exports a rigged, morphed, animated document, adds camera and light nodes, and
asserts the four counts, the message text and that the mesh and morph targets
still arrive; `mesh_only_glb_imports_without_a_skip_report` asserts the positive
path stays silent. The old importer had no report at all, so the counts and
message could not exist. Remaining: actually importing skins and animation
channels; the report is shown after the import completes, not in the file
dialog beforehand.

Wave 18 closed the largest remaining hole in **glTF import**: cameras and
lights. A `.glb` from any DCC carries the shot camera and the lighting, and the
importer turned both into anonymous null nodes, keeping only the names — the
same silent rewrite the earlier waves fixed, one level up from data loss. Node
`camera` references now become real cameras (perspective `yfov` converted to
degrees with `znear`/`zfar`, orthographic `ymag` mapped to `ortho_scale`), and
`KHR_lights_punctual` directional/point lights become real lights with the
authored colour and intensity, so an imported file renders with the shot it was
lit for. Invalid camera and light data now fails the import with a message
naming the node and the fix (missing projection block, out-of-range `yfov`,
non-positive `ymag`, dangling camera/light index, unknown or missing light
type), and the two things LeopardCraft genuinely cannot hold — spot lights and
the `range`/`aspectRatio` hints — are counted in `ImportReport` and named in its
summary instead of vanishing. This replaces the camera/light counters Wave 17
introduced; skins and animation clips are still reported as skipped.
`gltf::animation_export_tests::cameras_and_lights_are_imported_while_unsupported_data_is_reported`
adds two cameras, a directional, a point and a spot light to an exported GLB and
asserts the imported kinds, fov, clip planes, ortho scale, colours, intensities,
the spot refusal and the report text;
`camera_import_refuses_a_camera_without_projection_parameters` covers the
refusal. Before this wave those nodes imported as `Null` with no camera or
light data at all, so every one of those assertions failed. Remaining: spot
lights (needs a spotlight object), skin and animation import, and glTF light
units versus LeopardCraft intensities, which stay a documented approximation.

Wave 19 finished the punctual-light story: glTF spot lights and `range` were
the last light parameters the importer counted as skipped instead of honouring.
`LightKind` gained a `Spot { radius, inner_deg, outer_deg }` variant (cone down
the light's local -Z, angles in degrees), the path tracer samples it as a point
light gated by the cone with a smooth ramp between the inner and outer angles,
and the light inspector edits radius plus both angles. Import now converts
`innerConeAngle`/`outerConeAngle` from glTF radians to degrees, applies
`range` to the point/spot radius, and refuses a file whose cone breaks
`0 <= inner <= outer <= 90°` with a message naming the node. `ImportReport`
dropped its `spot_lights` and `light_ranges` counters because neither is
skipped any more; camera `aspectRatio` stays reported. glTF `range` is a
distance cutoff and our radius is a shadow/falloff radius, and glTF intensity
is photometric — both approximations are documented as clean-room, since
Cheetah3D's public docs list no glTF importer.
`gltf::animation_export_tests::cameras_and_spot_lights_are_imported_while_skips_are_reported`
now asserts the imported spot's cone in degrees (0.3 rad / 0.5 rad), its 7.5 m
range and its colour/intensity, and that the point light carries its 12 m range;
`spot_light_import_refuses_an_inverted_cone` covers the refusal, and
`lc_scene::format::tests::a_spot_light_roundtrips_through_native_format` proves
the new variant survives save/load. Before this wave a spot light imported as a
null node and both radii were the hard-coded 1 m, so those assertions could not
pass. Remaining: the tracer still ignores the radius for soft shadows (see the
idea bullet), and skin/animation import is unchanged.

Wave 20 took the top bounded item in the ranked gaps: a self-contained polygon
tool. **Triangulate** converts the selected n-gons into triangles, because until
now triangles only existed implicitly at render/export time and could not be
authored. `lc_geom::triangulate_faces` fans each selected polygon from its first
corner, keeps unselected polygons as n-gons, reuses every corner's point, UV
slot and material slot, and inherits creases, seams and pinned UV slots by
leaving those point/corner indices untouched; the new diagonals stay uncreased
and degenerate fans are skipped so the mesh still validates. Mesh Editing's
**Triangulate** button and `Tools -> Triangulate` share `mesh_triangulate_ready`:
polygon mode on editable meshes with a selected polygon of more than three
corners, which also makes the refusal messages meaningful, and one undo step
covers the whole action. `ops::tests::triangulate_splits_only_the_selected_polygons`
checks a box where one quad is selected: 7 polygons, identical positions and UV
slots, surviving crease/seam, untouched material slots, an unselected quad kept
at four corners, an unselected-index no-op and no crease on the fan diagonal.
`selection_tests::triangulate_is_polygon_mode_gated_and_one_undo_per_action`
checks the point-mode and empty-selection refusals with their exact messages,
the enabled menu command, the split result and undo. Before this wave there was
no triangulate operation, so both tests could not compile past the missing
function. Remaining: Cheetah3D's own split rule is not documented in the indexed
sources, so the fan is clean-room; a diagonal-choice option is recorded as an
idea.

Wave 21 shipped the **Flip Normal** polygon tool, the next cheap item in the
documented polygon-tool set (source 1). Cheetah3D lists the tool, but its manual
page is not among the pages indexed here, so acting on the current polygon
selection is a LeopardCraft clean-room choice. `lc_geom::flip_faces` reverses the
corner order of the selected polygons — the same reversal `Mesh::flip_normals`
applies to a whole mesh after mirroring — and leaves every other face, point, UV
slot, material slot, crease, seam and pinned UV slot untouched, because none of
those depend on corner order. Rendering, shading and glTF export read that
corner order as the face normal, so exactly the selected faces turn around. A
repeated face index flips once and out-of-range indices are ignored, so a
duplicated component selection cannot flip a face twice. Mesh Editing's **Flip
Normal** button and `Tools -> Flip Normal` share `mesh_flip_normal_ready`:
polygon mode on editable meshes with at least one selected polygon, with
point-mode and empty-selection refusals naming the reason. The action keeps the
component selection because face indices do not change, so the same command
flips the faces back, and one undo step restores the exact prior winding.
`ops::tests::flip_faces_reverses_only_the_selected_winding` flips one face of a
box (with a duplicate and an out-of-range index in the list), asserts the exact
reversed corner list, the unchanged neighbour, the negated face normal, untouched
positions and UVs, an empty-selection no-op and that flipping every face negates
the signed volume. `selection_tests::
flip_normal_reverses_the_selected_faces_and_one_undo_restores_them` covers the
point-mode and empty-selection no-ops (no undo entry, unchanged signature), the
enabled menu command, the exact reversed winding, the surviving face selection,
the `.lcscene` round-trip of the flipped mesh and the undo that restores the
original corner order. Remaining: the other eight documented polygon tools and a
viewport-click invocation.

Wave 22 shipped **Collapse** in its point-mode form: the selected points become
one point at their centroid. Cheetah3D lists Collapse in its polygon-tool set
(source 1) but the manual page is not among the pages indexed here, so the
selection rule is a LeopardCraft clean-room choice. `lc_geom::collapse_points`
wraps `Mesh::merge_points`, the topology helper now shared with the editor's
existing Weld command: the lowest selected index survives at the centroid, every
face that used a merged point uses it instead, adjacent duplicate corners are
folded so a quad that loses a corner becomes a triangle, and a face that still
repeats the point (or drops below three corners) is deleted instead of being
left pinched. Creases and seams follow the surviving indices, pins on UV slots no
face uses any more are dropped, and unused points are compacted away, so no mesh
attribute keeps a stale index. The op returns the survivor's index so the editor
can keep its selection valid; Mesh Editing's **Collapse** button and `Tools ->
Collapse` share `mesh_collapse_ready` (point mode on editable meshes with two
distinct selected points), refuse other modes with a reason, remap the component
selection to the merged vertex, and cover the whole action in one undo step.
`ops::tests::collapse_points_welds_the_selection_into_one_vertex` collapses three
corners of a box quad and asserts the centroid position, surviving points by
value, the removed face, the remapped crease and seam, the kept and dropped
pins, no repeated or out-of-range corner indices, an opposite-corner face being
deleted, and the empty/single/out-of-range no-ops.
`selection_tests::collapse_welds_selected_points_and_one_undo_restores_the_mesh`
covers the polygon-mode and empty-selection refusals (no undo entry, unchanged
signature), the enabled menu command, the reduced point count with the vertex
selection remapped, the `.lcscene` round-trip and the undo that restores the
exact prior points and faces. Remaining: the other seven documented polygon
tools, the manual's own collapse rules, and a viewport-click invocation.

Wave 23 shipped **Disconnect** in its polygon-mode form: the selected faces are
detached from the rest of the mesh without moving a vertex. Cheetah3D lists
Disconnect in its polygon-tool set (source 1) but the manual page is not among
the pages indexed here, so acting on the current polygon selection is a
LeopardCraft clean-room choice. `lc_geom::disconnect_faces` duplicates every
point that a selected face shares with an unselected face, points the selected
faces at the copies and leaves the unselected faces on the originals, so the
region becomes its own topological island with identical positions and winding.
Points used only by the selection stay shared between those faces, UV slots and
material slots are untouched (a corner keeps its UV value and its pin), and
creases and seams follow the faces: the original mark stays where an unselected
face still uses the edge and the cut gets the same mark on the duplicated indices
for the selected side. Every point stays referenced, so nothing is compacted and
polygon indices never change — the face selection stays exactly as it was and
now names the detached island. `lc_geom::can_disconnect` backs the shared
predicate: polygon mode on editable meshes with a selected face that still shares
a point with the rest of the mesh, so a whole-mesh or empty selection is refused
with a reason instead of reporting a no-op. Mesh Editing's **Disconnect** button
and `Tools -> Disconnect` use that predicate, and one undo step restores the
exact prior points and faces.
`ops::tests::disconnect_faces_detaches_the_selection_without_moving_it` detaches
one box face and asserts the copied positions with unchanged winding and UV
slots, untouched unselected polygons and UV array, the duplicated crease and seam
on both sides, a far crease that stays put, the surviving pin, no unused or
out-of-range indices, and the whole-mesh/empty/out-of-range no-ops.
`selection_tests::disconnect_detaches_selected_faces_and_one_undo_restores_the_mesh`
covers the point-mode, empty-selection and whole-mesh refusals (no undo entry,
unchanged signature), the enabled menu command, one undo step, the duplicated cut
points with identical positions, the untouched unselected faces, the still-valid
selection, the `.lcscene` round-trip and the undo that restores the exact prior
points and faces. Remaining: the other six documented polygon tools,
manual-confirmed selection modes, and a viewport-click invocation.

Wave 24 shipped **Cover** in its polygon-mode form: the selected faces read as
the ring around a hole, and every open boundary loop they border on every side
is closed with a new polygon. Cheetah3D lists Cover in its polygon-tool set
(source 1) but its manual page is not among the pages indexed here, so that
reading is a LeopardCraft clean-room choice; Fill Hole stays the other direction
— one selected border point in any mode fills every hole it touches.
`lc_geom::coverable_holes` collects the mesh boundary loops whose every edge is
an open boundary edge owned by a selected face, so a loop the selection only
partly borders, a closed mesh and an open chain that cannot be closed are left
out instead of corrupting the mesh. `lc_geom::cover_faces` hands those loops to
the existing `fill_holes` constructor, so each fill reuses the existing border
points in their existing order, reverses the winding to match its neighbours,
inherits the lowest bordering material slot and gets a planar UV projection over
its own 0..1 box; no existing point or polygon changes and the result is
watertight. The op returns the new face indices, so Mesh Editing's **Cover**
button and `Tools -> Cover` select the filled faces, and one undo step restores
the exact prior points and faces. The shared `mesh_cover_ready` predicate needs
polygon mode on editable meshes plus at least one completely ringed hole, and
refuses everything else with a reason.
`ops::tests::cover_faces_closes_the_hole_the_selection_borders` deletes one face
of a box, marks a crease, a seam and a pin, and asserts the hole is found only
for a complete ring, the new face reusing the border points, unchanged
positions, polygons and UV prefix, the inherited material slot, a watertight
edge set, the restored unit volume, and the partial-ring/empty/closed no-ops.
`selection_tests::cover_closes_the_selected_hole_and_one_undo_restores_the_mesh`
covers the point-mode, empty-selection and partial-ring refusals (no undo entry,
unchanged signature), the enabled menu command, one undo step, the untouched
existing faces and UV slots, the new face selected, the watertight result, the
`.lcscene` round-trip and the undo that restores the exact prior points and
faces. Remaining: the other five documented polygon tools, covering nested
holes in one pass, and a viewport-click invocation.

Wave 25 shipped **Create Polygon** in its point-mode form: the selected points
already on an open boundary become one or more new faces. Cheetah3D lists Create
Polygon in its polygon-tool set (source 1) but its manual page is not among the
pages indexed here, so that reading is a LeopardCraft clean-room choice.
`lc_geom::faceable_loops` keeps a mesh boundary loop only when every one of its
points is selected, no polygon already uses exactly that point set and its Newell
area is non-zero, so a partial run, a collinear run and a duplicated face are
left out while the remaining loops still become faces in one action. The loops
come from the mesh's own boundary walk, so the result never depends on the order
the points were clicked. `lc_geom::create_polygon_faces` appends one polygon per
loop over the existing point indices, reversed to wind with its neighbours, with
each corner taking its UV from the point's existing corner (and inheriting its
pin) and the polygon taking the lowest material slot bordering the loop; no point
is created, moved or renumbered and no existing polygon changes. The op returns
the new face indices, so Mesh Editing's **Create Polygon** button and `Tools ->
Create Polygon` select them and one undo step restores the exact prior points and
faces. The shared `mesh_create_polygon_ready` predicate needs point mode on
editable meshes plus at least one completely selected open boundary, and refuses
other modes or unusable selections with a reason.
`ops::tests::create_polygon_faces_closes_the_selected_boundary_loops` deletes box
faces to build one and then two holes and asserts the appended face(s) reuse
exactly the border points in adjacency order regardless of the click order, the
untouched positions, polygons and UV prefix, the inherited UV, pin and material
slot, a non-zero area with a watertight outward solid, the skipped partly
selected hole, and the empty/partial/closed refusals.
`selection_tests::create_polygon_appends_the_selected_face_and_one_undo_restores_the_mesh`
covers the polygon-mode, empty-selection and partial-run refusals (no undo entry,
unchanged signature), the enabled menu command, one undo step, the untouched
existing geometry, the new face selected, the watertight result, the `.lcscene`
round-trip and the undo that restores the exact prior points and faces.
Remaining: the other four documented polygon tools and a viewport-click point
picker.

Wave 26 shipped **Optimize** in its point-mode form: selected points that sit
within a tolerance of each other are welded into one point per cluster.
Cheetah3D lists Optimize in its polygon-tool set (source 1) but its manual page
is not among the pages indexed here, so welding only the selected clusters is a
LeopardCraft clean-room reading. `Mesh::weld_points` reuses the point-merge tail
Wave 22 added and now factors it out as `finish_point_merge`, shared with
`merge_points`: points are visited in index order and each joins the first
earlier survivor within the tolerance, so click order cannot change the result
and each survivor keeps its authored position. Faces keep the vertices they do
not merge, a face that loses an edge becomes a smaller polygon, and a face that
repeats a merged point is deleted; creases and seams follow the surviving
indices, pins on UV slots no face uses are dropped and unused points are compacted
away. `lc_geom::optimize_points` wraps it and returns the rebuilt indices of the
selected points, so Mesh Editing's **Optimize** button (with its Tolerance field)
and `Tools -> Optimize` leave the component selection on the survivors and one
undo step restores the exact prior points and faces. The shared
`mesh_optimize_ready` predicate needs point mode, a positive finite tolerance and
at least one mergeable cluster, and every refusal has its own message: not in
component mode, wrong mode, no positive tolerance, fewer than two points, or
nothing within tolerance.
`ops::tests::optimize_points_welds_only_nearby_selected_points` builds a two-quad
strip whose seam vertices are 0.4 mm apart and asserts the two duplicates fold
away, the faces are rebuilt on the survivors, the quad that collapses to a line is
removed, untouched positions and an untouched face stay byte-identical, the seam
becomes a shared edge, a crease and a surviving pin stay while a seam and a pin on
deleted geometry disappear, and every refusal (empty, single, zero/NaN tolerance,
points too far apart) returns nothing.
`selection_tests::optimize_welds_nearby_selected_points_and_one_undo_restores_the_mesh`
covers the wrong-mode, empty-selection, zero-tolerance and nothing-within-tolerance
refusals (no undo entry, unchanged signature), the enabled menu command, one undo
step, the welded result with the selection on the survivors, the `.lcscene`
round-trip and the undo that restores the exact prior points and faces. Remaining:
the other three documented polygon tools, a spatial-hash tolerance search instead
of the per-cluster scan, and a whole-mesh (ungrouped) optimize mode.

Wave 27 shipped **Point Slide** in its point-mode form: each selected point moves a
fraction of the way toward the centroid of the points its own edges connect it to.
Cheetah3D lists Point Slide in its polygon-tool set (source 1) but its manual page
is not among the pages indexed here, so the neighbour-centroid reading is a
LeopardCraft clean-room choice. `Mesh::slide_points` builds the adjacency from the
mesh's polygon edges (sorted and de-duplicated per point, so selection order and
duplicate selections cannot change the result), measures every centroid from the
positions *before* the move, and rewrites only the selected positions; a point with
exactly two neighbours therefore lands on the segment midpoint at a fraction of 1
and half way there at 0.5. Nothing else is touched — no corner record, winding,
material slot, UV slot, crease, seam or pin changes and no index moves, so the
component selection stays valid and still names the slid points.
`lc_geom::slide_points` wraps the method (returning `None` when nothing moves) and
`lc_geom::can_slide_points` backs the **Point Slide** button in Mesh Editing, its
Fraction field (default 0.5) and `Tools -> Point Slide`. The shared
`mesh_point_slide_ready` predicate needs component mode, point mode, a fraction
above zero and at most one, and at least one selected point that an edge connects
to something; each refusal (not in component mode, wrong mode, fraction outside
0..=1, a zero fraction, empty selection, no selected point with a neighbour) has
its own message and leaves history depth and the document signature untouched.
`ops::tests::slide_points_moves_only_the_selection_toward_its_neighbours`
asserts a two-neighbour point reaches the exact segment midpoint at 1 and stops at
(0.25, 0, 0.25) at 0.5, a three-neighbour point lands on a third of the way to its
neighbour centroid, only the selected position changes while every other point and
all face corner records stay byte-identical, a duplicate selection slides once,
and empty, out-of-range, negative, above-one and NaN fractions and a point with no
edge all return nothing while `can_slide_points` agrees.
`selection_tests::point_slide_moves_the_selection_and_one_undo_restores_the_mesh`
covers the wrong-mode, empty-selection, out-of-range fraction, zero-fraction and
stale-selection refusals (no undo entry, unchanged signature), the enabled menu
command, one undo step, the moved position with every other point and face
unchanged, the selection still on the slid point, the `.lcscene` round-trip and the
undo that restores the exact prior points and faces. Remaining: scalpel and clone,
the other two documented polygon tools.

A tracker audit on 2026-11-23 re-checked every unchecked box against the code.
Nothing unchecked has shipped in the meantime: `relax_uvs` still splits charts
on seams only (`crates/lc-geom/src/uv.rs`), `MorphAction`
(`crates/lc-ui/src/app.rs`) offers Record/Capture/Restore/Remove but no
in-place base update, the editor still has no viewport joint picking, the two
remaining polygon tools have neither geometry ops nor menu entries, and
`lc-anim` has no key filter or document-level takes. The one stale entry was
the glTF report bullet in §5, which still listed spot lights and light `range`
as skipped after Wave 19 implemented both; it now lists the three fields
`ImportReport` actually carries. Thirty-two unchecked boxes remain, and they
are the work left rather than duplicates of shipped waves; three gaps live in
section prose instead of a box — viewport joint picking (§2), Spline IK and
Cheetah3D-style constraint tags (§2b deliberate limits), and a drag-in-viewport
Shell invocation (§6b prose, mirrored in the parity doc).

**Still open in Animation Core:** key filtering, document-level takes and
per-take fps remain. Constant, Linear, Smooth and Spline interpolation and the
per-side Tangent Type commands are implemented for scalar, tag/material,
morph-weight and quaternion keys, with Spline scalar/morph handles, one-step
undo and native round-trip; selected-key and whole per-object take factor
retiming is also implemented. Rotation keys remain quaternion samples without
scalar handles; each side's tangent type selects its flat, secant or
Catmull-Rom cubic control.

## 1. Animation Core

Make animation data general enough to drive more than the nine transform channels,
and make the curve editor behave like a real key editor.

**Data model**

- [ ] Replace the flat `AnimProperty` enum (`crates/lc-anim/src/lib.rs`) with a
      channel address that names target + property + component, so material
      colour and modifier parameters can be animated without growing the enum
      (morph weights already use the dedicated `morph_weights` curve map). Keep
      old `.lcscene` files loadable with a custom
      `Deserialize` that maps the current snake_case variant names.
- [x] Add quaternion rotation channels with shortest-path interpolation and a
      deterministic Euler/quaternion conversion at key time.
      `AnimationClip::apply_transform` now prefers the quaternion track and only
      falls back to legacy Euler curves for old documents.
- [x] Make per-key tangents authoritative. A `tangent_manual` flag keeps
      hand-authored incoming/outgoing slopes through insert/remove edits; auto
      tangents still update for non-manual keys. Native load/save and the
      document signature include the slopes.
- [ ] Move takes to document level instead of per node; the Takes tab currently
      states takes are stored per object.
- [x] Define one property registry used by Key Transforms, the curve editor and
      the evaluator, so a new animatable property is registered once. The
      registry also declares channel scope and applicability (object, camera,
      light, tag, material).

**Editor**

- [x] Select, box-select, move, copy/paste and delete individual keys, each as
      one undo command through the existing animation dispatch. Marquee bounds
      match both frame and displayed value; additive selection uses shift or
      command.
- [x] Retime selected keys in time as one undo command, pivoting on the
      earliest selected key, and retime a whole per-object take around 0s from
      the Takes tab. Positive factors preserve fractional times, reject
      collisions/negative results atomically, move scalar/tag/material/morph/
      rotation keys together, rescale hand-authored tangents by incident
      segment duration, re-derive automatic tangents, and restore times and
      slopes through one undo/redo entry and native `.lcscene` serialization.
- [x] Drag Spline tangent handles in the F-curve editor for selected scalar and
      morph-weight keys. A drag edits one side, becomes one undo step, and marks
      the key as hand-authored; re-recording returns it to automatic tangents.
- [x] Per-key Constant, Linear, Smooth and Spline interpolation plus the
      `Animation -> Tangent -> Type` and left/right Tangent Type commands in the
      curve editor. Object, tag, material, morph-weight and quaternion keys can
      be selected, edited and set independently; Smooth re-derives automatic
      tangents and Spline preserves hand-authored slopes.
- [x] Expose every registry channel for the active object, tag or material as
      selectable F-curve rows with `Key Channel` recording; mesh objects also
      list one dynamic Morph Weight row per target.
- [ ] Key filter (all / selected / transform only).
- [ ] Edit playback range, per-take fps and loop mode; `Playback` currently has
      one global range and fps.
- [ ] Group the channel list by target object in the timeline.

**Evaluation and tests**

- [ ] Sample each channel once per frame into a scratch buffer instead of
      re-querying the clip channel-by-channel.
- [x] Tests: key-edit undo/redo on scalar and quaternion keys, and
      deterministic loop sampling.
- [ ] Tests: old-document deserialisation (an actual legacy document on disk,
      not just a clip built with the old enum) and rotation continuity across
      ±180°.
- [x] Tests: hand-authored tangent sampling, neighbour-edit survival, tangent
      undo/redo and native round-trip.
- [x] Tests: selected-key retiming, tag/material and whole-take paths,
      fractional times, collision rejection, curve resampling, undo/redo and
      retimed native round-trip.
- [x] Tests: resampled Constant/Linear/Smooth/Spline shapes, automatic tangent
      re-derivation versus preserved manual Spline slopes, mixed scalar/morph/
      quaternion selection undo/redo, tag/material Tangent Type coverage, and
      interpolation/tangent-type native round-trip.

## 2. Weight Painting and Binding

Weight painting and skin binding are implemented. Evaluation already supports
four influences (`skin_positions_indexed`, `crates/lc-scene/src/eval.rs`); the
editor now authors, binds, validates and preserves those weights.

- [x] Add a weight brush mode to the viewport that paints `Skin::weights`
      (`crates/lc-scene/src/scene.rs`) for the selected joint, with radius,
      strength and constant/linear/smooth falloff.
- [x] Brush modes add / subtract / replace / smooth, with 4-influence pruning and
      normalisation after each stroke.
- [x] Active-joint selection from the joint list in the Skinning panel, plus a
      heat-map overlay for the active joint. Viewport joint picking is still open.
- [x] One undo command per stroke that coalesces the whole drag, following the
      existing all-or-nothing `SetShapeCmd` pattern.
- [x] Sanitise skins on load in `crates/lc-scene/src/format.rs`: finite
      non-negative weights, indices in range, lengths matching the vertex count,
      weights normalised.
- [x] Bind workflow: bind to chosen joints only through a per-joint checklist;
      only those joints enter the skin.
- [x] Proximity auto-bind for the chosen joints: deterministic inverse-distance
      weights to bone segments, normalized, capped by Max influences, with
      Dropoff and Strength controls in the Skinning panel.
- [x] Rebinding preserves existing positive weights for joints that remain
      selected; uncovered vertices are auto-weighted when enabled or stay
      unweighted otherwise.
- [x] Zero-total vertices stay unweighted and evaluate at rest rather than
      falling back to joint 0.
- [x] Tests: falloff determinism, normalisation and pruning, stroke undo, format
      round-trip, selected-joint binding, proximity deformation and
      weight-preserving rebind plus brush undo after rebind.

The brush works on modifier-free editable meshes because skin arrays are indexed
by the evaluated mesh. Joint selection is a list in the Skinning panel, not a
viewport pick. Chosen-joint binding, proximity auto-weights and weight-preserving
rebind are complete; viewport joint picking is separate work.

## 2b. Inverse Kinematics

Cheetah3D's 4.1 release notes document a 2D IK solver and an IK baking command
in the Take Manager; its public feature list also advertises Spline IK and IK
Angle/Point Constraint tags. The community IK Handle workflow reports a default
Length of 2 and target/pole object controls, while the 8.x solver internals and
exact chain-counting rules are not publicly documented. The implementation here
is therefore a clean-room equivalent with explicit semantics, not a claim about
Cheetah3D's internal solver.

- [x] Add `IkChain` to the scene graph on the tip Joint. `target` and optional
      `pole` are node ids; `chain_length` counts consecutive movable Joint
      ancestors above the tip; `iterations`, `tolerance` and `damping` bound the
      solve. Existing `.lcscene` files load with no IK chain.
- [x] Solve enabled chains during `Document::evaluate` before the tag-constraint
      pass. The tip origin is the goal. A two-joint chain uses the law of
      cosines and the pole node to choose the elbow plane; one-joint and longer
      chains use damped CCD, with an optional pole-plane preference after
      convergence.
- [x] Clamp unreachable targets to the minimum/maximum reach without stretching
      bones, skip zero-length bones and missing ancestors without NaNs, and cap
      iterations so a solve always terminates.
- [x] Add the **Inverse Kinematics** inspector for Joints: create/remove a chain,
      choose target and pole, set chain length, iterations, tolerance, damping
      and enabled state, and bake the playback range.
- [x] Bake every frame in the playback range (up to a 10,000-frame safety cap)
      as rotation keys on the movable joints through one `SetAnimationsCmd` undo
      entry, so disabling IK leaves the solved pose in normal playback and glTF
      export animation.
- [x] Tests: analytic two-bone reach and rotation changes, pole side, multi-bone
      CCD reach, skinned-mesh deformation, unreachable reach clamping with
      finite transforms, zero-length/missing-ancestor safety, current-frame and
      range bake playback, and NodeProps IK round-trip. Existing skin/bind,
      rebind, brush and undo tests remain green.
- [x] Add `JointLimits` to every Joint with an enabled flag and min/max local
      XYZ Euler degrees. Limits are normalized on load, serialized in the
      native format, included in `NodeProps` undo and dirty signatures, and
      editable through the Joint inspector's **Joint Rotation Limits** section.
- [x] Apply limits to direct/animated local rotation before IK and clamp after
      each correction in both analytic two-bone and damped CCD paths. Zero-width
      axes lock, reversed finite bounds swap, missing limits leave the pose
      unchanged, and every degenerate/unreachable path stays finite.
- [x] Tests: analytic and CCD limit enforcement, direct-rotation clamping with
      IK disabled, zero-width locking, native round-trip and NodeProps undo.

Deliberate limits: there is no Spline IK, no IK Limits/Angle/Point or broader
constraint authoring, and no viewport joint picking. The per-joint limits are a
clean-room local-Euler approximation, not Cheetah3D's tag-based
heading/pitch/bank controls or its documented 3D-solver-only behavior; there is
no stiffness control. Targets and poles are node origins, not arbitrary control
surfaces. A bake uses the global playback range rather than document-level
takes, and IK settings are not a glTF interchange feature; baked rotation keys
are exported as ordinary animation. The two-bone pole is exact; the CCD pole is
a plane preference after convergence, not a hard twist constraint.

## 3. UV Workflow

`automatic_unwrap` in `crates/lc-geom/src/uv.rs` charts by normal angle, breaks
charts along authored seams and shelf-packs the result; `set_uv_point` remains
the only direct UV edit. Build the rest of a real UV toolset on top of that.

- [x] Explicit seam edges: mark/unmark from selected mesh edges with **Toggle
      Seam** (Mesh Editing Edge mode and `Selection -> Toggle Seam`), persist them
      on the mesh as canonical point pairs, draw them green in the 3D and UV
      views, inherit them through subdivision, and cut unwrap charts along them
      instead of relying only on the normal-angle threshold. Seams follow point
      compaction, welding, merging and native save/load; old documents load with
      no seams.
- [ ] Seam authoring from the UV view itself, plus select-all-seams, clear-seams
      and seam-loop selection. The UV view currently displays seams only.
- [x] Pinned UVs: pin/unpin the UV point selected in the UV view with **Toggle
      Pinned UV** (UV panel toolbar and `Selection -> Toggle Pinned UV`), one
      undo step per toggle and disabled without a selected UV point. Pins are
      document state (`Mesh::pinned_uvs`) that survives native save/load, point
      compaction, weld and merge, draws yellow in the UV view and is counted in
      its footer. Unwrap, relax and normalize keep a pinned slot at its exact
      authored coordinate while everything else re-solves; each pinned chart is
      best-fit translated and uniformly scaled onto its pins first.
- [x] Chart-aware relaxation. `relax_uvs` now relaxes per chart: the graph is
      split at every seam, so the two sides of an authored cut keep separate UV
      values instead of being averaged onto one shared point value, and seam or
      mesh-boundary corners are held when the pin-boundary option is on (the
      editor's Relax command passes `true`). Pins stay hard per-corner
      constraints, and the no-pin path still re-fits the layout into 0..1.
- [ ] Relax follow-up: the chart splitter divides on seams only, not on the
      unwrap's normal-angle charts, and Relax does not re-align or re-pack charts
      after smoothing.
- [ ] Multi-point UV pin selection. The UV view selects one point, so pins are
      toggled one point at a time; Cheetah3D's UV editor supports multi-selection,
      and its pinned set also survives later transforms of the pinned points.
- [ ] Chart selection and chart move/rotate/scale, plus packing controls
      (padding, rotation, alignment) over the existing shelf packer.
- [ ] UV editing operations: island selection, weld/split UV vertices,
      align/distribute, flip, rotate 90°, numeric UV entry.
- [ ] Multiple UV sets per mesh (UV0/UV1) with a selector, usable by texture and
      lightmap slots.
- [ ] Viewport UV checker overlay and per-face distortion stats so unwrap quality
      is visible.
- [x] Tests: seam-driven unwrap determinism, per-chart relax stability with
      seams as borders, and direct UV point edit undo plus invalid-point and
      non-editable-mesh refusals.
- [ ] Tests: non-overlapping chart packing (the packer is only asserted to stay
      inside 0..1) and multi-set round-trip.

## 4. Morph Targets

Morph authoring is implemented. The Base workflow matches Cheetah3D's Morph tag:
record the rest shape, capture moved meshes as named targets, restore the base
and blend with weights. Weight curves are sampled by evaluation, exported, and
listed as dynamic Morph Weight rows in the F-curve editor. The manual does not
document a numeric weight range or morph-specific interpolation, so the 0–1
clamp and shared F-curve interpolation are clean-room choices.

- [x] Data model: `MorphSet` with a recorded base and `MorphTarget { name,
      deltas: Vec<Vec3>, weight }` on mesh nodes, with target deltas tied to the
      base vertex count.
- [x] Document format and load-time validation in `crates/lc-scene/src/format.rs`,
      including finite/clamped weights, unique names, topology filtering and
      clearing morph data from non-mesh nodes.
- [x] One documented evaluation order in `crates/lc-scene/src/eval.rs`:
      recorded base → weighted morph deltas → modifiers → skinning → node
      transform. Recording against the editable mesh (before modifier topology
      changes) is what makes this order correct.
- [x] UI: target list with weight sliders, capture/remove/rename, Record Base and
      Restore Base, with disabled actions and messages when the mesh is not
      editable or no longer matches the base.
- [x] Undo commands for capture/remove/rename and weight edits (`SetMorphCmd`),
      plus shape undo for restoring the base.
- [x] Morph weight curves on `AnimationClip` (`morph_weights`), sampled during
      evaluation, exposed through the Animation Core registry as per-target
      F-curve rows and exported as glTF `weights` animation channels.
- [x] Tests: weighted delta application and clamping, deterministic evaluation,
      format round-trip, undo and mismatched-length handling; F-curve row
      authoring, interpolation/range clamping, native round-trip and undo/redo
      of morph-weight keys.

### Remaining morph work

- [x] Expose morph weight tracks in the Animation Core registry and F-curve
      editor; the clip data and evaluation path support them.
- [ ] Update the recorded base in place when the rest mesh changes.
- [x] Import morph targets, names and weights from glTF/GLB. `primitive.targets`
      POSITION offsets become a `MorphSet` whose base is the imported rest mesh,
      names come from `mesh.extras.targetNames` (or a target's `extras.name`),
      and default weights come from `mesh.weights` with node `weights`
      overriding them. Files the importer cannot merge are refused with a
      message naming the mesh or node: primitives that disagree on target
      count, a name list of the wrong length, a target accessor whose element
      count does not match the primitive, and a weights array that does not
      match the target count.
- [ ] Expose morph targets in the object browser or a dedicated tag-style panel.

## 5. glTF Animation and Skinning Interchange

Export is implemented (`crates/lc-io/src/gltf.rs`): glTF/GLB carries bind-pose
skins, named morph targets and a merged Take of node TRS and morph-weight
animation. Import reads meshes, transforms, core PBR values, a mesh's morph base,
target names, offsets and default weights, cameras, and all three punctual light
types with their cones and falloff distances; `skins` and `animations` are still
skipped and reported.

- [x] Export `skins`: inverse bind matrices as a MAT4 accessor, joint node
      indices, and `skin` on skinned mesh nodes, mapping `Skin::joints` to glTF
      node indices.
- [x] Export `animations`: shared-time-base samplers and channels for
      translation/rotation/scale and morph weights. Uniform STEP/LINEAR channels
      are preserved; all-Smooth/Spline scalar channels use sampled CUBICSPLINE
      tangents, while mixed channels and smooth rotations are sampled LINEAR.
- [x] Export morph targets: named `targets` with POSITION/NORMAL deltas on every
      primitive, `mesh.extras.targetNames`, and node default `weights`.
- [x] Export bind-pose joint TRS recovered from the stored inverse-bind
      matrices, and a bind-pose mesh without skinning or morph deformation.
- [x] Non-fatal skips for incomplete/topology-mismatched skins and morph bases
      that no longer match the exported rest mesh or require a modifier stack.
- [x] Import glTF cameras and directional/point lights. Node `camera`
      references become `NodeKind::Camera` (perspective `yfov` in radians →
      `fov_y_deg`, `znear`/`zfar`; orthographic `ymag` → `ortho_scale =
      2 * ymag`), and `KHR_lights_punctual` directional/point lights become
      `NodeKind::Light` with their authored colour and intensity. Light colour
      and intensity are copied verbatim even though glTF's photometric units
      differ from LeopardCraft's intensity scale — a clean-room approximation,
      since Cheetah3D documents no glTF importer. A camera with no usable
      projection block, an out-of-range `yfov`, a non-positive `ymag`, a missing
      camera/light definition and an unknown or missing light type all fail with
      a message naming the node and the fix.
- [x] Report skipped authored glTF data instead of dropping it silently. The
      importer returns an `ImportReport` holding the counts of skinned mesh
      nodes, animation clips and camera `aspectRatio` hints (aspect comes from
      the render output) — the only authored data left unread after Wave 19
      added spot lights and `range` — plus a summary naming what was skipped and
      what the user can do about it. The editor
      appends that summary to the Open and Import status lines, `open_scene`
      returns it as `import_report`, and headless/MCP startup prints it to
      stderr. A file with no such data still imports as before with an empty
      report.
- [ ] Show the skipped-data summary before the scene is replaced (pre-import
      confirmation or a dismissible banner), so the user can cancel instead of
      undoing the import after reading the status line.
- [ ] Import `skins` and `animations` into `Skin` and `AnimationClip`,
      including CUBICSPLINE tangents and clip duration → fps. Their presence is
      reported on import; morph targets, names, default weights, cameras and
      directional/point lights already import.
- [x] Import glTF spot lights and honour their `range`. A `KHR_lights_punctual`
      spot light becomes a LeopardCraft spot light carrying the authored
      `innerConeAngle`/`outerConeAngle` converted from radians to the degrees
      the document stores, and a point or spot `range` becomes that light's
      `radius` falloff/shadow distance instead of the fixed 1 m default. A cone
      that violates `0 <= inner <= outer <= 90°` fails the import with a message
      naming the node and the rule. Clean-room mapping: Cheetah3D documents no
      glTF importer, glTF `range` is a distance cutoff while our radius is a
      shadow/falloff radius, and glTF intensity is photometric while ours is an
      arbitrary scale.
- [ ] Feed a light's radius (imported from glTF `range`) into soft-shadow
      sampling in the tracer; today it is stored on the light and shown in the
      inspector, while the tracer still samples point and spot lights exactly at
      their position.
- [ ] Give spot lights their own object-browser icon; they currently share the
      point-light glyph, and the inspector is what distinguishes them.
- [x] Real-export parse tests for GLB and `.gltf` that assert skin joints,
      inverse binds, JOINTS_0/WEIGHTS_0 bounds, named morph POSITION/NORMAL
      deltas, accessor min/max, animation samplers/channels, STEP/LINEAR values,
      bind-pose joint TRS, a rig-free regression and non-fatal malformed skips.
- [ ] Round-trip import/export tests for skin and animation data, plus
      malformed accessors and out-of-range joints. Morph round-trip is covered.
- [x] Update the README interchange table and limitations list.

## 6. Modifier and subdivision parity

Cheetah3D documents Twist, Shear, Bulge, Spherify and two further subdivision
schemes (Stam-Loop and Linear) beside the existing Bend and Taper deformers
and the Catmull-Clark modifier. The manual pages describe shared Cage/Type
controls and per-modifier Angle, Distance, Radius and Strength parameters.
Its subdivision modifier also documents edge creases that sharpen the result
under Catmull-Clark and Stam-Loop.

- [x] Add Twist, Shear, Bulge and Spherify stack modifiers
      (`crates/lc-scene/src/geometry.rs`) with deterministic geometry in
      `crates/lc-geom/src/ops.rs`.
- [x] Add a Linear subdivision scheme beside Catmull-Clark, with shared edge
      midpoints and watertight output (`crates/lc-geom/src/subdiv.rs`).
- [x] Serialize the subdivision scheme with a Catmull-Clark default so old
      `.lcscene` files load unchanged.
- [x] Expose the new modifiers and the scheme selector in the modifier stack
      inspector.
- [x] Tests: deformer shape/finite extremes, distinct subdivision schemes,
      watertight linear topology, modifier serde round-trip and legacy
      scheme default.
- [ ] Shared Cage/Type controls (`Constrained`, `Unconstrained`, `Cage`).
- [x] Add the Stam-Loop scheme beside Catmull-Clark and Linear. It
triangulates the cage and applies Loop edge and vertex rules, making mixed
quad/triangle cages measurably distinct from both other schemes.
- [x] Add per-mesh edge creases as canonical point pairs, with a **Toggle
Crease** action for the active edge selection in Mesh Editing, a blue crease
overlay in Edge mode, and crease inheritance through Catmull-Clark and
Stam-Loop subdivision levels.
- [x] Honour creases with midpoint edge points, the two-crease 3/4 + 1/8 + 1/8
vertex rule and a pinned corner rule for three or more creases. Linear
subdivision ignores crease sharpness.
- [x] Serialize creases and the Stam-Loop scheme; normalize crease data on
load and default legacy documents to no creases and Catmull-Clark.
- [x] Tests: three-way scheme distinctness on a mixed cage, crease sharpening
under both smooth schemes at multiple iteration levels, linear independence,
edge-mode toggle undo, native round-trip and legacy document loading.
- [ ] Exact Stam-Loop hybrid rules from Stam's published scheme, crease
weights, partial subdivision and crease display outside Edge mode.
- [ ] Displacement, formula, magnet, polygon-reduction, Laplacian smooth and
      wrap modifiers.

## 6b. Polygon tools

Cheetah3D advertises a large polygon-tool set (source 1) and documents Fill
Hole's modes and click workflow (source 38). Unlike seams, pins and creases,
these tools carry no new document state: each one changes topology only, so it
is a self-contained geometry operation with its own undo step.

- [x] Fill Hole: `open_holes` finds the boundary loops under a component
      selection and `fill_holes` closes each with a single n-gon, reversed to
      wind with its neighbours. Available in point, edge and polygon mode from
      Mesh Editing's **Fill Hole** button and `Tools -> Fill Hole`; enabled only
      when every selected editable mesh has a selected point on an open border.
      One undo step per action, the panel reports how many holes sit under the
      selection, and creases, seams and pinned UVs survive the fill.
- [x] Split: `extract_faces` copies the selected polygons into a new mesh,
      remapping only the points and UVs they use and carrying the creases,
      seams and pins inside the selection. Mesh Editing's **Split** button and
      `Tools -> Split` add one object per selected mesh beside its source in a
      single undo step, copying the source's transform, flags, material slots,
      tags and per-tag clips. Polygon mode on editable meshes only, as source 39
      documents, with the panel reporting the polygon count.
- [x] Shell: `shell_region` groups polygons across edges within the documented
      Angle, and `shell_faces` offsets the region along averaged normals,
      appends the reversed copy and rim quads and applies the documented Inside
      and Quantize properties. Mesh Editing's thickness/Inside/Angle/Quantize
      row and `Tools -> Shell` share one predicate: polygon mode on editable
      meshes with a selected polygon and a non-zero distance. One undo step per
      action, the panel reports the region size, and creases, seams, pins, split
      parts and a filled hole survive the shell and a save/load.
- [x] Triangulate: `triangulate_faces` fans each selected n-gon into
      `corners - 2` triangles from its first corner and leaves unselected
      polygons as n-gons, so only the authored selection changes. Corners keep
      their point, UV slot and material slot, and creases, seams and pinned UV
      slots survive because they are point/corner indices; the new fan
      diagonals carry no crease or seam, and degenerate fans are skipped so the
      result still validates. Mesh Editing's **Triangulate** button and
      `Tools -> Triangulate` share one rule: polygon mode on editable meshes with
      at least one selected polygon that has more than three corners, so the
      command is never a silent no-op. One undo step per action. The manual's
      Triangulate page is not among the sources indexed here (source 1 lists the
      tool), so the fan-from-the-first-corner split is a LeopardCraft clean-room
      choice, not a documented Cheetah3D rule.
- [x] Flip Normal: `flip_faces` reverses each selected polygon's corner order
      and leaves every other face alone, which turns exactly those faces' normals
      around for rendering, shading and export. Point and UV slots, material
      slots, creases, seams and pins are unaffected because none of them depend
      on corner order; a repeated index flips once and out-of-range indices are
      ignored. Mesh Editing's **Flip Normal** button and `Tools -> Flip Normal`
      share one predicate — polygon mode on editable meshes with at least one
      selected polygon — and refuse point mode or an empty selection with a
      reason. The face selection survives the flip because indices do not
      change, so the command toggles back, and one undo step restores the exact
      prior winding. The manual page is not among the indexed sources (source 1
      lists the tool), so acting on the current selection is clean-room.
- [x] Collapse: `collapse_points` merges the selected points into the lowest
      selected index, moved to their centroid, through `Mesh::merge_points` —
      the same topology helper the Weld command now uses. Faces that used a
      merged point are rebuilt around it, an adjacent duplicate corner folds
      away so a quad becomes a triangle, and a face that repeats the point or
      drops below three corners is removed instead of left pinched. Creases,
      seams and pins follow the surviving indices (marks and pins on topology
      that disappears are dropped) and unused points are compacted, so nothing
      keeps a stale index. Mesh Editing's **Collapse** button and `Tools ->
      Collapse` share one predicate — point mode on editable meshes with two
      distinct selected points — and refuse other modes with a reason. The op
      reports the survivor so the component selection follows the merged
      vertex, and one undo step restores the exact prior points and faces. The
      manual page is not among the indexed sources (source 1 lists the tool), so
      the selection rule is clean-room.
- [x] Disconnect: `disconnect_faces` duplicates every point shared by a
      selected and an unselected face, points the selected faces at the copies
      and leaves the unselected faces on the originals, so the region becomes
      its own topological island with identical positions and winding. Interior
      points of the selection stay shared, UV and material slots are untouched
      (a corner keeps its UV value and its pin), and creases and seams follow
      their faces — the original mark stays for the unselected side and the cut
      gains the same mark on the duplicated indices. Every point stays used, so
      nothing is compacted and face indices never change; the selection
      therefore stays valid and names the detached island. Mesh Editing's
      **Disconnect** button and `Tools -> Disconnect` share one predicate —
      polygon mode on editable meshes with a face that borders unselected
      geometry — so whole-mesh and empty selections are refused with a reason.
      One undo step restores the exact prior points and faces. The manual page
      is not among the indexed sources (source 1 lists the tool), so the
      polygon-mode reading is clean-room.
- [x] Cover: `coverable_holes` finds every mesh boundary loop whose edges are
      all open boundary edges owned by a selected face, and `cover_faces` fills
      those loops through the existing `fill_holes` constructor — existing
      border points in their existing order, reversed winding to match the
      neighbours, the lowest bordering material slot and a planar UV projection
      for the new face. No existing point or polygon changes, the mesh becomes
      watertight at the hole, and loops the selection only partly borders, open
      chains that cannot be closed and closed meshes are skipped. The op returns
      the new face indices so the selection names them; Mesh Editing's **Cover**
      button and `Tools -> Cover` share one predicate — polygon mode on editable
      meshes with at least one completely ringed hole — and refuse other modes
      or selections with a reason. One undo step restores the exact prior points
      and faces. The manual page is not among the indexed sources (source 1
      lists the tool), so reading the selection as the hole's ring is
      clean-room.
- [x] Create Polygon: `faceable_loops` keeps every mesh boundary loop whose
      points are all selected, that no polygon already covers and whose Newell
      area is non-zero, and `create_polygon_faces` appends one polygon per loop
      over the existing point indices — reversed to wind with the neighbours,
      with each corner inheriting its point's UV and pin and the polygon taking
      the lowest bordering material slot. No point is created, moved or
      renumbered and no existing polygon changes. Loops come from the mesh's
      boundary walk, so click order cannot reorder the face; partial runs,
      collinear runs and duplicate faces are skipped while valid runs still
      become faces. The op returns the new face indices so the selection names
      them; Mesh Editing's **Create Polygon** button and `Tools -> Create
      Polygon` share one predicate — point mode on editable meshes with at least
      one completely selected open boundary — and refuse other modes or unusable
      selections with a reason. One undo step restores the exact prior points and
      faces. The manual page is not among the indexed sources (source 1 lists
      the tool), so the point-mode reading is clean-room.
- [x] Optimize: `weld_points` groups only the selected points that sit within
      the Tolerance field of each other, visiting them in index order so the
      result does not depend on click order, and keeps each survivor at its
      authored position. It shares the point-merge tail Wave 22 added
      (`finish_point_merge`): faces are rebuilt on the survivors, a face that
      loses an edge shrinks, a face that repeats a merged point is removed,
      creases and seams follow the surviving indices, pins on UV slots no face
      uses are dropped and unused points are compacted. The op returns the
      rebuilt selected indices so the component selection stays valid; Mesh
      Editing's **Optimize** button, its Tolerance field and `Tools -> Optimize`
      share one predicate — point mode, a positive finite tolerance and at least
      one mergeable cluster — and each refusal case (not in component mode,
      wrong mode, no positive tolerance, fewer than two points, nothing within
      tolerance) has its own message. One undo step restores the exact prior
      points and faces. The manual page is not among the indexed sources (source
      1 lists the tool), so the selected-cluster reading is clean-room.
- [x] Point Slide: `Mesh::slide_points` moves each selected point a fraction of
      the way toward the centroid of the points its polygon edges connect it to,
      measuring every centroid from the positions before the move so neither the
      selection order nor a duplicate selection can change the result. A point
      with exactly two neighbours lands on the segment midpoint at a fraction of
      1. Only selected positions change: no corner record, winding, material
      slot, UV slot, crease, seam or pin is touched and no index moves, so the
      component selection stays valid. The shared `mesh_point_slide_ready`
      predicate needs component mode, point mode, a fraction above zero and at
      most one, and a selected point with a neighbour; the **Point Slide**
      button, its Fraction field (default 0.5) and `Tools -> Point Slide` all
      share it, each refusal case has its own message, and one undo step
      restores the exact prior positions and faces. The manual page is not among
      the indexed sources (source 1 lists the tool), so the neighbour-centroid
      reading is clean-room.
- [ ] Remaining polygon tools: scalpel and clone.
- [ ] Offer a triangulation option for the split direction (shortest diagonal or
      quad pair) instead of always fanning from the first corner.
- [ ] Holes that enclose an island, and a click-in-viewport invocation instead of
      acting on the current component selection.

## 7. MCP automation

`leopardcraft --mcp` exposes the document over the Model Context Protocol stdio
transport, complementing the headless CLI and the separate Cheetah3D scripting
gap. The server can be used while the editor is closed; it owns the loaded
document.

- [x] Add the `lc-mcp` crate with a line-delimited JSON-RPC stdio server, CLI
      entry point and `--scene` startup document.
- [x] Support both the `2025-06-18` `initialize` handshake and the
      `2026-07-28` handshake-free revision, including `server/discover`,
      cacheable result fields and server metadata.
- [x] Expose read-only scene tools: `get_scene`, `list_objects`, `get_object`
      and `list_materials`, with object lookup by id or exact name.
- [x] Expose undoable edit tools: `create_primitive` with all primitive
      parameters and local transforms, `set_transform`, `rename_object`,
      `delete_object` and `set_object_properties`.
- [x] Expose file and output tools: `save_scene`, `open_scene`, `new_scene`,
      `export_scene` and CPU path-traced `render_image` (PNG/EXR), plus `undo`
      and `redo`.
- [x] Publish `leopardcraft://scene` and `leopardcraft://scene/summary`
      resources.
- [x] Tests: handshake and discovery, handshake-free calls, deterministic tool
      schemas, create/transform/rename/delete/properties, undo/redo, resources,
      save/open/export round-trips, PNG render output, protocol errors and the
      stdio serve loop.
- [x] Make the launch cwd-independent: `.mcp.json` resolves
      `${CLAUDE_PROJECT_DIR:-.}/scripts/mcp-server.sh`, and the wrapper pins the
      workspace manifest/package, changes to the repository root and builds a
      missing or stale binary before starting the server.
- [x] Add a real-process smoke test that starts the binary outside the
      repository, completes initialize + tools/list, calls representative
      tools, checks the advertised schemas and compares the tool inventory with
      `AGENTS.md`; a second check launches through `.mcp.json` with Claude Code's
      variable expansion.
- [ ] Attach to a running editor window so an agent can manipulate the live
      session and its selection instead of a separate in-memory document.
- [ ] Modifier-stack, material-graph, animation and mesh/UV/component authoring
      tools.
- [ ] Streamable HTTP transport, resource subscriptions and prompt templates.

## 8. Agent workflow

Keep the repository easy to navigate and verify for AI coding agents. The
canonical instructions live in `AGENTS.md`; `CLAUDE.md` points Claude-based
agents at the same file so there is one source of truth. Multi-agent lane
briefs and the orchestration playbook live under `docs/agents/`.

- [x] Add `AGENTS.md` with ground rules, commands, a workspace/architecture
      map, task-to-file recipes, conventions, testing guidance and the MCP
      quick start.
- [x] Add `scripts/check.sh`, which runs formatting, clippy with `-D warnings`,
      the whole test suite and `git diff --check`, reporting the full result.
- [x] Add a project-scoped `.mcp.json` so Claude Code can launch the
      LeopardCraft MCP server from the project root or a subdirectory; other
      clients have the absolute wrapper path.
- [x] Add `app/tests/mcp_smoke.rs`, a real-binary integration test covering the
      handshake, tool calls, schema/inventory checks and the project config
      launch from outside the repository.
- [x] Run the MCP smoke test explicitly in `scripts/check.sh` as well as through
      `cargo test --workspace`, so the single agent gate catches protocol and
      launch regressions. CI inherits the workspace test run.
- [x] Document the agent entry points and the cwd-independent launcher in the
      README.
- [x] Health-check the exact launch command with Claude Code 2.1.327 via a
      disposable local-scope entry; it connected. Claude Code discovers the
      project entry from the repository root and a subdirectory, and the
      automated smoke test covers the same command without requiring Claude
      Code in CI.
- [x] Split the repository into six non-overlapping agent lanes with scoped
      briefs under `docs/agents/` covering owned paths, entry points,
      invariants, cross-lane interfaces and lane checks.
- [x] Document the orchestration model in `docs/agents/README.md`: lane
      dependencies, shared contracts, task packets, parallel rounds, handoff
      format and integration order.
- [x] Add project-scoped Claude Code subagents in `.claude/agents/` mapping the
      six lanes, each loading the root instructions, playbook and lane brief.
- [x] Add `scripts/check-lane.sh <lane>` for per-lane formatting, clippy and
      tests, leaving `scripts/check.sh` as the full integration gate.
- [ ] Approve the project-scoped server in an interactive Claude Code session
      (user consent), then confirm `/mcp` lists the LeopardCraft tools.

## Suggested order

1. **Animation Core** — finish key filtering and document-level takes; the
   channel model, quaternion rotation, manual tangents and selected-key/take
   retiming already cover interpolation, morph animation and timing changes.
2. **Character rigging** and **UV Workflow** — weight authoring, tip-origin IK
   with take baking and clean-room joint limits are in place; viewport joint
   picking, spline IK, Cheetah3D-style constraint tags and UV tools remain
   user-visible gaps.
3. **glTF import and interpolation parity** — export covers skins, morphs and
   animation and native morph-weight F-curve rows are keyable; import remains
   the interchange gap, and exact Cheetah3D linear/smooth/spline and weight-range
   semantics are not reproduced.
