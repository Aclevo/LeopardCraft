# LeopardCraft — next improvements

To-do list for the five agreed workstreams. Boxes stay unchecked until an item's
full scope lands. Each item names the code that has to change so the list can be
worked top to bottom without re-deriving the plan.

**Roadmap:** the remaining unchecked items are grouped into missions G30–G39
in [`docs/agents/goals/ROADMAP.md`](docs/agents/goals/ROADMAP.md). G30 (morph
base update and spot-light icon), G31 (morph presentation and playback
controls), G32 (document-level takes with per-take fps) and G33 (channel
addresses and sampled evaluation) are complete; G34 is the next startable
mission.

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
pass. Remaining: skin and animation import are unchanged.

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

Wave 28 closed two tracker items across lanes. The F-curve editor's **Filter**
selector now lists and draws all registry channels, only channels that own a
selected key on the current target, or only translation/rotation/scale rows; the
same filter gates canvas picking and box selection, and it is view state that
never edits the document or history.
`selection_tests::fcurve_key_filter_shows_all_selected_and_transform_rows`
checks the three row sets on a camera, the selected-key narrowing and that
filtering leaves history depth unchanged. On the geometry side,
`uv::tests::automatic_unwrap_packs_disconnected_charts_without_overlap` unwraps
four disconnected quads and asserts every chart stays inside 0..1 and no pair
of chart bounds overlaps, closing the packer-coverage gap. `lc-anim` also gained
`rotation_continuity_across_the_half_turn_uses_the_short_path`, which
interpolates +170° to -170° with Linear and Smooth keys and asserts unit
quaternions, a continuous orientation on the 170–180° arc and a ±180°
midpoint; the same box's legacy-on-disk document test remained open until
Wave 29.

Wave 29 closed the four remaining tracker slices in one path-disjoint batch.
`lc-scene::format::tests::legacy_on_disk_fixture_loads_with_documented_defaults_and_evaluates`
loads the checked-in `crates/lc-scene/tests/fixtures/legacy_wave1.lcscene`
through `load_native`: the file has the pre-quaternion scalar-track shape, no
newer node fields and a Subdivision modifier without a scheme, and the test
asserts the documented defaults plus an animated evaluation. `relax_uvs` now
cuts charts on the same normal-angle borders as `automatic_unwrap` in addition
to seams, holds pins exactly and shelf-packs the smoothed charts back into 0..1
without overlap. The CPU tracer samples a point or spot light's authored radius
as a spherical shell, so a larger radius widens the penumbra; zero and
non-finite radii keep the old hard-shadow path, and splitting a render into
passes is now bit-identical to one pass. Viewport joint handles let a click on a
bound joint set the Skinning panel's active joint, heat map and weight brush;
unbound joints have no handle and empty space preserves the chosen joint. GUI
appearance was not visually verified.

A tracker audit on 2026-11-23 re-checked every unchecked box against the code.
Waves 28 and 29 then closed the F-curve key filter, non-overlapping UV packing,
the legacy-document-on-disk test, the Relax follow-ups and viewport joint
picking. At that audit, `MorphAction` (`crates/lc-ui/src/app.rs`) offered
Record/Capture/Restore/Remove but no in-place base update, the two remaining
polygon tools have neither geometry ops nor menu entries, and `lc-anim` has no
document-level takes. Two gaps live in section prose instead of a box: Spline IK
and Cheetah3D-style constraint tags (§2b deliberate limits), and a
drag-in-viewport Shell invocation (§6b prose, mirrored in the parity doc).

Wave 30 completed the G30 editor close-out. `MorphAction::UpdateBase` re-records
the live rest mesh and rebases every topology-matching target delta so each
target keeps its absolute posed shape; the inspector exposes **Update Base**,
one `SetMorphCmd` undo step restores the prior base and deltas, and refusals
cover no base, topology changes and an unchanged mesh. Spot lights now have a
distinct cone glyph in the object browser instead of sharing the point-light
icon.

Wave 31 completed G31. The Object Browser gained a **Morphs** tab that lists
every mesh's targets under its owner with weight and derived On/Off state; a
row click selects the object and switches the inspector to Object, and the list
reads each `MorphSet` directly instead of copying it. Playback start, end and
loop are edited through centralized setters used by the timeline Range menu, the
F-curve header and the Animation menu; bounds stay finite and ordered
(`0..=end`), and range or loop changes leave the document signature and history
untouched. Editor tests cover the rows and owner selection, clamp and refusal
paths, loop toggling, history depth and signature stability.

Wave 32 completed G32. `Document` now owns named takes with an active take and
independent fps; the active take's node, tag and material clips are mirrored
into the live animation view, and switching/creating/deleting a take restores
the matching clips. Pre-take `.lcscene` files migrate into one default take
without changing evaluation, while save, load and the semantic signature keep
the active take in sync. The Takes tab is a document take manager (new,
rename, duplicate, delete, activate, fps) with one undo entry per action and
playback retimed from the active take's fps. glTF export names and samples the
active take and carries its name/fps in `extras`; import reads that metadata or
keeps a default take. Editor and document tests cover switching, per-take fps
against the playback range, invalid-value clamping/refusal, undo boundaries and
round-trip signature stability.

**Still open in Animation Core:** the channel-address rewrite, grouped channel
list and scratch sampling; the key filter, global playback range/loop and
document takes now ship. Constant, Linear, Smooth and Spline interpolation and the
per-side Tangent Type commands are implemented for scalar, tag/material,
morph-weight and quaternion keys, with Spline scalar/morph handles, one-step
undo and native round-trip; selected-key and active-take clip factor
retiming is also implemented. Rotation keys remain quaternion samples without
scalar handles; each side's tangent type selects its flat, secant or
Catmull-Rom cubic control.

**Tracker wave 35** closed six open boxes. Image Texture nodes gained a UV set
index and the renderer now interpolates every UV set at the hit, so texture and
lightmap slots can sample UV0 or any extra set. Open/Import stage a glTF that
would lose camera `aspectRatio` hints and show an **Import Report** before the
scene changes. The glTF importer now reads skins (joints, inverse binds,
`JOINTS_0`/`WEIGHTS_0`) and animation channels (STEP/LINEAR/CUBICSPLINE,
translation/rotation/scale/morph weights, one take per animation, duration at
the take's fps), with round-trip and malformed-data tests. Bend, Taper, Twist,
Shear, Bulge and Spherify share the documented Cheetah3D Type/Cage controls,
and Triangulate offers Shortest Diagonal and Quad Pair instead of a fixed fan.

**Wave 34 (G33)** replaced the flat scalar `AnimProperty` key with a channel
address: keys now name a target, a registry channel and a component. The
`AnimChannel` registry remains the only source of scope, component count, label
and applicability, and `ChannelAddress::new` refuses undeclared components. The
loader maps old snake_case property names to owner addresses and keeps legacy
Euler rotation curves in a compatibility field, so old `.lcscene` files still
evaluate and round-trip. Evaluation now builds one scratch buffer per clip per
frame: `AnimationClip::sample_channels` evaluates each addressed curve once and
node/tag/material consumers read all components from that buffer instead of
re-querying curves per component. The F-curve list groups rows under their
target object, tag or material, and glTF export resolves addressed node tracks
while naming any channel it cannot place in an `AnimationExportReport`.

## 1. Animation Core

Make animation data general enough to drive more than the nine transform channels,
and make the curve editor behave like a real key editor.

**Data model**

- [x] Replace the flat `AnimProperty` enum (`crates/lc-anim/src/lib.rs`) with a
      channel address that names target + property + component, so material
      colour and modifier parameters can be animated without growing the enum
      (morph weights already use the dedicated `morph_weights` curve map).
      `ChannelAddress { target, channel, component }` is validated by the
      `AnimChannel` registry; `ChannelTarget` is `Owner`, `Object { id }`,
      `Tag { node, index }` or `Material { id }`. Old `.lcscene` files load
      through a custom `Deserialize` that maps the snake_case `AnimProperty`
      names to owner addresses and keeps legacy Euler rotation curves in a
      compatibility field; save writes the new address form.
- [x] Add quaternion rotation channels with shortest-path interpolation and a
      deterministic Euler/quaternion conversion at key time.
      `AnimationClip::apply_transform` now prefers the quaternion track and only
      falls back to legacy Euler curves for old documents.
- [x] Make per-key tangents authoritative. A `tangent_manual` flag keeps
      hand-authored incoming/outgoing slopes through insert/remove edits; auto
      tangents still update for non-manual keys. Native load/save and the
      document signature include the slopes.
- [x] Move takes to document level instead of per node. `Document` owns named
      takes, an active index and per-take fps; node, tag and material clips are
      snapshotted per take and restored when it activates. Old files migrate
      into one default take, and the Takes tab edits the document take list.
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
      earliest selected key, and retime the selected object's active-take clip
      around 0s from the Takes tab. Positive factors preserve fractional times, reject
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
- [x] Key filter (all / selected / transform only). The F-curve header's
      **Filter** selector drives the row list, drawn curves, point picking and
      box selection: all registry channels, channels that own a selected key,
      or transforms only. It is view state and never edits the document.
- [x] Edit the global playback start/end range and loop mode from the timeline
      Range menu and the F-curve header. Both bounds clamp to a finite, ordered
      `0..=end` range; playback edits never mutate the document or history.
- [x] Per-take fps. The active take's fps drives `Playback`, the timeline range
      and glTF sampling; finite values clamp to 1–240 and non-finite values are
      refused. The legacy `DocumentSettings::fps` field mirrors the active take.
- [x] Group the channel list by target object in the timeline. The F-curve row
      list shows one group per target: the current object, tag or material
      lists every filtered registry row, and the primary object's other
      animatable tags and the selected material appear with the channels that
      own keys. Clicking a row in another group focuses that target; grouping
      and filters remain view state and never edit history.

**Evaluation and tests**

- [x] Sample each channel once per frame into a scratch buffer instead of
      re-querying the clip channel-by-channel. `AnimationClip::sample_channels`
      fills a `ChannelSamples` buffer with one curve evaluation per addressed
      channel (plus the quaternion and legacy Euler tracks); evaluation stores
      node and per-tag buffers once and reads translation, scale, rotation,
      camera FOV, light intensity/colour and Copy Location weight from them
      across both constraint passes. Material roughness/metallic share one
      buffer per evaluation call.
- [x] Tests: key-edit undo/redo on scalar and quaternion keys, and
      deterministic loop sampling.
- [x] Tests: old-document deserialisation from an actual legacy document on
      disk (`crates/lc-scene/tests/fixtures/legacy_wave1.lcscene`, loaded
      through `load_native`) and rotation continuity across ±180°.
- [x] Tests: hand-authored tangent sampling, neighbour-edit survival, tangent
      undo/redo and native round-trip.
- [x] Tests: selected-key retiming, tag/material and whole-take paths,
      fractional times, collision rejection, curve resampling, undo/redo and
      retimed native round-trip.
- [x] Tests: document take creation, switching node/tag/material clips,
      per-take fps driving playback, invalid fps/name clamping and refusal,
      one-step take undo, legacy migration and take round-trip signature.
- [x] Tests: resampled Constant/Linear/Smooth/Spline shapes, automatic tangent
      re-derivation versus preserved manual Spline slopes, mixed scalar/morph/
      quaternion selection undo/redo, tag/material Tangent Type coverage, and
      interpolation/tangent-type native round-trip.
- [x] Tests: channel-address registry validation, legacy `property` JSON
      deserialization and on-disk fixture addressing, sample-once buffer reuse
      and explicit-target isolation, addressed native round-trip with signature
      stability, grouped F-curve rows across object/tag/material targets with
      filters, and glTF skipped-channel reporting.

## 2. Weight Painting and Binding

Weight painting and skin binding are implemented. Evaluation already supports
four influences (`skin_positions_indexed`, `crates/lc-scene/src/eval.rs`); the
editor now authors, binds, validates and preserves those weights.

- [x] Add a weight brush mode to the viewport that paints `Skin::weights`
      (`crates/lc-scene/src/scene.rs`) for the selected joint, with radius,
      strength and constant/linear/smooth falloff.
- [x] Brush modes add / subtract / replace / smooth, with 4-influence pruning and
      normalisation after each stroke.
- [x] Active-joint selection from the joint list in the Skinning panel or by
      clicking a bound joint's viewport handle, plus a heat-map overlay for the
      active joint. Unbound joints are not handle targets, and an empty click
      preserves the chosen joint.
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
by the evaluated mesh. Joint selection is the Skinning panel list or a viewport
click on a bound joint's screen-space handle. Chosen-joint binding, proximity
auto-weights, weight-preserving rebind and viewport joint picking are complete.

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
- [x] Spline IK: `IkChain.spline` names a Spline node and the Joint inspector's
      **Spline** row selects it next to the point target.
      `solve_spline_chain` samples the curve into a world-space polyline and aims
      each bone at the curve point one bone length further along from that
      joint's projection onto the curve, twice per solve; rotating only keeps
      every bone length exact, so the chain follows the guide as a chain of
      chords. The tip reports its distance from the curve, a guide shorter than
      the chain clamps to its end, joint limits clamp after every correction,
      the solve is deterministic, IK bakes the resulting rotations like any
      other chain, and a missing or non-spline node falls back to the point
      target. The field is serde-defaulted, so older documents load without it.
      Clean-room note: the public pages advertise Spline IK but define neither
      its solver nor a twist/up-vector control.
      Tests: `spline_ik_follows_the_guide_curve_without_stretching`,
      `spline_chain_reports_curve_error_and_short_curve_clamp`,
      `spline_ik_falls_back_to_the_point_target_for_a_non_spline_node`,
      `spline_solve_is_deterministic`,
      `bake_includes_a_spline_chain_whose_point_target_is_gone` and
      `ik_chain_reads_legacy_json_without_a_spline_target`.
- [x] IK stiffness and the constraint tags: `IkChain.stiffness` (serde default
      1, skipped in the file while full) blends every solved joint back toward
      its pre-solve rotation once per solve, so 1 is the full solve, 0 keeps the
      rest pose and 0.5 lands halfway. One shared step serves the analytic
      two-bone/pole, CCD and spline paths, limits are re-clamped after the blend,
      and skinning, playback and Bake IK all read the blended pose. Four
      clean-room constraint tags drive any node through the existing tag pass:
      **IK Point** lerps the owner's origin toward a target by weight, **IK
      Angle** keeps the owner's local +X axis inside a `max_degrees` cone around
      the direction to a target (it only pulls), **Orient** slerps the owner's
      world rotation toward a target's, and **Parent** blends a target's world
      position and rotation. Blend weights measure against the pose the solver
      produced, so the evaluator's two ordering passes are idempotent; tags still
      apply in list order, so a later Clamp Rotation wins. The Joint inspector
      gains the **Stiffness** slider and the Tags section gains Target/Weight and
      Target/Cone rows plus Add Tag buttons, and `baked_joint_ids` keys a joint
      whose only driver is a tag. Cheetah3D documents a tag stiffness where one
      means the joint does not bend; this chain-level blend is the opposite
      convention, chosen so existing chains keep their behaviour, and the manual
      publishes no tag solver to match.
      Tests: `stiffness_lands_between_the_rest_pose_and_a_full_solve`,
      `stiffness_softens_ccd_and_spline_chains`,
      `ik_point_constraint_pulls_the_owner_toward_its_target`,
      `ik_angle_constraint_keeps_the_aim_inside_its_cone`,
      `orient_and_parent_constraints_blend_by_weight`,
      `constraint_driven_joint_bakes_and_evaluates_deterministically` and
      `constraint_tags_read_legacy_json_and_keep_defaults_out_of_the_file`.

Deliberate limits: the per-joint limits and the four constraint tags are
clean-room approximations, not Cheetah3D's tag-based heading/pitch/bank controls
or its documented 3D-solver-only behavior, and the manual publishes no solver
for its IK tags. Stiffness here is a chain-level blend toward the rest pose where
1 is a full solve, while Cheetah3D's tag stiffness of one means the joint does
not bend. IK Angle constrains the local +X aim axis, IK Point translates its
owner (on a joint that moves the joint itself, not the bones around it), and
Orient/Parent copy world transforms rather than binding to a control surface.
Spline IK follows a guide curve as a chain of chords with no twist or up-vector
control. Targets and poles are node origins, not arbitrary control surfaces. A
bake uses the global playback range rather than document-level takes, and IK
settings are not a glTF interchange feature; baked rotation keys are exported as
ordinary animation. The two-bone pole is exact; the CCD pole is a plane
preference after convergence, not a hard twist constraint.

**G34** completed the UV authoring suite. Geometry gained chart
derivation and selection, chart transform and packing with padding/rotation/
alignment, slot-level weld/split/align/distribute/flip/rotate, per-face
distortion measurement and additional UV sets stored parallel to the shared
slot list. The document schema adds `Mesh::extra_uvs` with legacy defaults and
load repair. The UV view now selects several points, authors seams and pins,
runs every chart and slot operation, switches sets and shows checker and
distortion overlays. glTF carries extra sets as `TEXCOORD_1`… in both
directions, and OBJ names the sets it cannot write.

## 3. UV Workflow

`automatic_unwrap` in `crates/lc-geom/src/uv.rs` charts by normal angle, breaks
charts along authored seams and shelf-packs the result. The UV view now authors
seams and pins, selects several points and islands, transforms and packs charts,
runs slot-level UV operations, switches between UV sets and overlays a checker
and per-face distortion readout. All UV authoring is undoable through the
quantity-preserving shape command and round-trips through native files.

- [x] Explicit seam edges: mark/unmark from selected mesh edges with **Toggle
      Seam** (Mesh Editing Edge mode and `Selection -> Toggle Seam`), persist them
      on the mesh as canonical point pairs, draw them green in the 3D and UV
      views, inherit them through subdivision, and cut unwrap charts along them
      instead of relying only on the normal-angle threshold. Seams follow point
      compaction, welding, merging and native save/load; old documents load with
      no seams.
- [x] Seam authoring from the UV view itself, plus select-all-seams, clear-seams
      and seam-loop selection. With two adjacent UV points selected, **Toggle
      Seam** toggles the mesh edge between them as one undo step; **All Seams**
      selects the UV points on every seam, **Seam Loop** walks the connected
      seam chain from the selection (or all seams when nothing is selected) and
      **Clear Seams** removes the list in one undo step. The same commands are
      in `Tools -> UV Editing` and share the UV-edit predicate with the panel.
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
- [x] Relax follow-up: the chart splitter divides on the unwrap's normal-angle
      borders as well as seams, and Relax shelf-packs the smoothed charts back
      into 0..1 without overlap while pins stay exact. `relax_splits_charts_on_
      the_unwraps_normal_angle_borders` covers a folded edge and
      `relax_repacks_disconnected_charts_without_overlap` covers the packer; the
      seam and pin tests remain green.
- [x] Multi-point UV pin selection. The UV view keeps a selection set, with
      shift/command-click toggling a point and Select All/None/Island filling
      it. **Toggle Pinned UV** pins or unpins every selected point as one undo
      step, and editing or transforming a pinned point keeps its pin flag.
- [x] Chart selection and chart move/rotate/scale, plus packing controls
      (padding, rotation, alignment) over the existing shelf packer.
      `uv_charts`/`charts_for_uv_slots` derive seams-and-angle charts from the
      mesh, **Island** selects a chart from any of its points, and the chart
      Transform fields move/rotate/scale the selected charts around their
      combined centre. `pack_uv_charts` shelf-packs the selection (or every
      chart) with padding, optional 90° chart rotation and bottom-left, top-left
      or centred alignment; charts containing pinned points are skipped and
      counted in the report so a pin never moves.
- [x] UV editing operations: island selection, weld/split UV vertices,
      align/distribute, flip, rotate 90°, numeric UV entry. `weld_uv_slots`
      merges selected slots that share a mesh point (compacting every set and
      the pin list), `split_uv_slots` gives each selected shared slot one slot
      per corner, and align/distribute/flip/rotate operate on the active set's
      coordinates. Numeric entry sets every selected slot to one U/V pair. Each
      operation has a UV panel button and a `Tools -> UV Editing` entry, and one
      undo step covers the whole edit.
- [x] Multiple UV sets per mesh (UV0/UV1) with a selector, usable by texture and
      lightmap slots. `Mesh::extra_uvs` holds additional sets parallel to the
      shared slot list (one coordinate per slot), with `add_uv_set`,
      `remove_uv_set`, `uv_set`, `uv_set_mut`, `uv_value` and `set_uv_value`.
      The UV panel's set selector chooses the active set; projections write the
      active set, direct editing/chart/slot operations target it, and Unwrap,
      Relax and Normalize stay UV0 tools with a refusal message on another set.
      Weld and split change the shared slot topology, so they affect every set.
      Old documents load with one UV0 set; native save/load preserves all sets
      and the signature includes them. glTF exports and imports extra sets as
      `TEXCOORD_1`…; OBJ carries one set and writes a skipped-set comment.
      Image Texture nodes carry a UV set index; the CPU renderer interpolates
      every set at the hit, so a texture can sample UV0 or any extra set and a
      lightmap is a texture node bound to UV1.
- [x] Viewport UV checker overlay and per-face distortion stats so unwrap quality
      is visible. The UV view's **Checker** toggle densifies the checker
      background, **Distortion** tints each face by its normalized UV/surface
      area ratio, and the footer reports mean/max stretch and the number of
      faces over 2×. `uv_face_distortion` and `uv_distortion_stats` normalize
      against the mesh's overall UV density so an even layout reports 1. The
      overlays are view state and never touch the document or history.
- [x] Tests: seam-driven unwrap determinism, per-chart relax stability with
      seams as borders, and direct UV point edit undo plus invalid-point and
      non-editable-mesh refusals.
- [x] Tests: non-overlapping chart packing.
      `automatic_unwrap_packs_disconnected_charts_without_overlap` unwraps four
      disconnected quads and asserts each chart stays inside 0..1 and no pair of
      chart bounds overlaps.
- [x] Bind material texture and lightmap slots to a chosen UV set.
      `ShaderNodeKind::Texture` now carries a `uv_set` (serde-defaulted to UV0),
      the texture node samples `ShadingContext::uv_for_set`, and a missing set
      falls back to UV0 instead of a stale coordinate. `Triangle` carries every
      extra set, `Hit` interpolates them, and the shaded context receives the
      slice, so texture and lightmap slots can sample any authored set. The
      material inspector exposes the UV set field, and the node's `uv_set` is
      part of the graph signature. Tests: `texture_uv_set_selects_the_coordinate`
      (UV1 samples one texel, missing UV1 falls back, signature changes) and
      `hits_carry_interpolated_additional_uv_sets` (renderer interpolation).
- [x] Tests: multi-set round-trip. `additional_uv_sets_roundtrip_and_damaged_sets_are_repaired_on_load`
      covers native save/load, signature stability, short-set repair and the
      legacy one-set load; `glb_roundtrips_additional_uv_sets` proves GLB writes
      `TEXCOORD_1` and imports both sets back; `obj_export_reports_skipped_additional_uv_sets`
      checks the OBJ skipped-set comment. Chart transforms, packing with pins,
      split/weld, seam authoring, multi-point pins and the checker/distortion
      view state are covered by focused geometry and editor tests.

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
- [x] Update the recorded base in place when the rest mesh changes.
- [x] Import morph targets, names and weights from glTF/GLB. `primitive.targets`
      POSITION offsets become a `MorphSet` whose base is the imported rest mesh,
      names come from `mesh.extras.targetNames` (or a target's `extras.name`),
      and default weights come from `mesh.weights` with node `weights`
      overriding them. Files the importer cannot merge are refused with a
      message naming the mesh or node: primitives that disagree on target
      count, a name list of the wrong length, a target accessor whose element
      count does not match the primitive, and a weights array that does not
      match the target count.
- [x] Expose morph targets in the Object Browser's **Morphs** tab: each target
      is listed under its owning mesh with weight and On/Off state, and selecting
      a row selects the owner without copying the morph data.

## 5. glTF Animation and Skinning Interchange

Export is implemented (`crates/lc-io/src/gltf.rs`): glTF/GLB carries bind-pose
skins, named morph targets and a merged Take of node TRS and morph-weight
animation. Import reads meshes, transforms, core PBR values, a mesh's morph base,
target names, offsets and default weights, cameras, all three punctual light
types with their cones and falloff distances, skins and animation channels.
`ImportReport` now only names camera `aspectRatio` hints, and the editor shows
that report before replacing the scene.

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
- [x] Show the skipped-data summary before the scene is replaced. Open and
      Import stage the parsed document when `ImportReport::summary()` is
      non-empty; `PendingImport` holds it, the **Import Report** window shows the
      file name and summary with **Open Anyway**/**Import Anyway** and
      **Cancel**, and Escape cancels. Cancel leaves the document, history and
      status untouched; confirming applies the staged document (Open loads it,
      Import uses one `ReplaceDocumentCmd` undo step). Tests:
      `gltf_open_stages_skipped_data_until_confirmed` and
      `gltf_import_stages_a_report_and_one_undo_restores_the_scene`.
- [x] Import `skins` and `animations` into `Skin` and `AnimationClip`,
      including CUBICSPLINE tangents and clip duration → fps. A node's `skin`
      resolves its joint nodes and inverse-bind matrices (identity when the
      accessor is absent), reads `JOINTS_0`/`WEIGHTS_0` per vertex, normalizes
      four influences and refuses an out-of-range joint or missing attributes.
      Animation channels for translation/rotation/scale/weights import STEP,
      LINEAR and CUBICSPLINE; cubic scalar tangents become hand-authored Spline
      slopes, cubic rotation becomes Smooth (quaternion keys have no tangent
      handles), the first animation fills take 0 and later animations become
      extra takes, and each take's frame range is the clip duration at its fps.
      `ImportReport` no longer counts skins or clips.
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
- [x] Feed a light's radius (imported from glTF `range`) into soft-shadow
      sampling in the tracer. Point and spot lights sample a deterministic
      spherical shell; `tracer::tests::light_radius_creates_a_deterministic_
      penumbra_and_stays_finite` checks the hard-shadow core, the wider penumbra,
      and finite zero/NaN/infinite radii.
- [x] Give spot lights their own object-browser icon; they currently share the
      point-light glyph, and the inspector is what distinguishes them.
- [x] Real-export parse tests for GLB and `.gltf` that assert skin joints,
      inverse binds, JOINTS_0/WEIGHTS_0 bounds, named morph POSITION/NORMAL
      deltas, accessor min/max, animation samplers/channels, STEP/LINEAR values,
      bind-pose joint TRS, a rig-free regression and non-fatal malformed skips.
- [x] Round-trip import/export tests for skin and animation data, plus
      malformed accessors and out-of-range joints. Morph round-trip is covered.
      `skins_and_animation_roundtrip_through_import` checks joints, normalized
      weights, a Constant translation curve, a quaternion rotation, a
      morph-weight curve and the duration-derived frame range;
      `cubic_animation_import_keeps_hand_authored_tangents` checks Spline keys
      with manual tangents; `skin_import_refuses_a_joint_index_outside_the_skin`
      and `animation_import_refuses_a_missing_accessor` cover the failure paths.
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
- [x] Shared Cage/Type controls (`Constrained`, `Unconstrained`, `Cage`).
      Bend, Taper, Twist, Shear, Bulge and Spherify carry a serde-defaulted
      `DeformCage { limit, extent }`; `deform_with_cage` applies the documented
      manual semantics: Constrained clamps a point to the scaled cage before
      deforming and reuses that border displacement outside, Cage deforms only
      points inside the cage, and Unconstrained ignores it. The inspector shows
      one Type combo and the Cage extent field per deformer.
      `deformer_cage_limits_control_which_vertices_move` proves Cage leaves an
      all-outside mesh untouched, Constrained differs from Unconstrained, and
      all three stay finite and valid.
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
- [x] Fractional crease weights. `Mesh::crease_weights` stores strengths in
      `(0, 1)` separately from full creases; `set_crease_weight` moves an edge
      between the lists at 0/1, and subdivision blends the smooth and sharp
      edge, vertex and Loop rules by that weight (two-crease vertices use the
      smallest incident weight). Both child edges inherit the weight. The Mesh
      Editing edge panel exposes a 0–1 Weight field and **Set Crease Weight**,
      and native load normalizes/clamps the data.
      Tests: `catmull_clark_crease_weights_blend_edge_and_vertex_rules_exactly`,
      `stam_loop_blends_fractional_crease_corners_between_the_two_rules`, the
      fractional native round-trip and the UI weight/undo test.
- [x] Partial subdivision. `subdivide_faces` builds a patch from the selected
      faces, pins its boundary onto the original boundary segments and threads
      the new collinear boundary points into bordering unselected faces, so the
      border is watertight and unselected vertices stay fixed. Any of the three
      schemes, UVs/extra sets, creases, seams, weights and material slots
      survive; **Subdivide** in Mesh Editing and `Tools -> Subdivide` expose
      levels and scheme with one undo step.
      Test: `partial_subdivision_is_watertight_and_preserves_the_unselected_region`
      across all three schemes, plus the UI one-undo test.
- [x] Creases visible outside Edge mode: the viewport draws full creases in
      blue and fractional creases in a weight-blended blue in point, polygon
      and object modes. Test: `creased_edges_are_drawn_outside_edge_mode`.
- [x] Exact Stam-Loop hybrid rules from Stam's published scheme. Implemented
      2026-11-24 (sources 44 and 45) as one operator per iteration: every n-gon
      is split into n quads around its centroid — three for a triangle, four for
      a quad — and the old vertices, edge points and face points take the rule
      of the region they sit in: Catmull-Clark masks on quad-only rings, Loop's
      vertex and edge masks on triangle-only rings, and the paper's mixed mask
      where the two meet (`alpha = 1 / (1 + n_e/2 + n_f/4)` on the vertex,
      `alpha/2` on each edge midpoint and `alpha/4` on each face centroid, solved
      over the actual ring so the weights sum to one). A mixed cage therefore
      stays all quads. An edge between a quad and a triangle takes the mean of
      the two edge rules, because the paper's boundary mask survives only as
      figure text; creases, fractional weights, seams, extra UV sets and
      material slots keep their existing handling. The paper's optional
      vertex-correction pass (Table 1) is not applied: it exists to recover the
      Catmull-Clark and Loop masks from a uniform averaging pass, and those
      masks are applied exactly per region here. Tests:
      `stam_loop_reduces_to_catmull_clark_on_quad_and_ngon_cages` (exact
      equality), `stam_loop_uses_loop_rules_on_a_triangle_only_cage`,
      `stam_loop_positions_each_region_with_its_own_rule` (mask values, region
      split, affine invariance) and the updated
      `stam_loop_is_distinct_on_a_mixed_quad_triangle_cage` (all quads).
- [x] Displacement, formula, magnet, polygon-reduction, Laplacian smooth and
      wrap modifiers. `Modifier::Displacement` offsets points along their
      area-weighted normals by deterministic value noise (amplitude, frequency,
      seed). `Modifier::Formula` evaluates one expression per axis in the
      point's rest `x`/`y`/`z` with `+ - * / ^`, parentheses, `pi`/`e` and a
      fixed function set; an empty expression keeps its coordinate, a
      non-finite result keeps it too, and a malformed expression is refused by
      the add/update predicate with the axis and syntax problem named.
      `Modifier::Magnet` pulls points toward the bounds centre by a signed
      strength with a smoothstep radius falloff, `Modifier::LaplacianSmooth`
      runs a uniform umbrella smoothing with iterations, strength and a
      boundary hold, `Modifier::PolygonReduction` collapses the shortest
      triangle edge at its midpoint until the face count reaches its ratio
      (stop when no triangle edge remains), and `Modifier::Wrap` rotates the
      object around a target mesh's centre line by an angle spread along an
      axis. Boolean and Wrap share the evaluated operand path in `eval.rs`, so
      Wrap takes a real target mesh and a self-target is a no-op; every new
      parameter is serialized with a default so hand-written and older stacks
      load; the inspector edits every parameter, picks Wrap's target in its own
      **Add Wrap** menu and rejects a formula that does not parse; and all six
      are mesh-only. Tests:
      `displacement_moves_points_along_normals_deterministically`,
      `formula_evaluates_per_axis_and_rejects_malformed_input`,
      `magnet_pulls_nearer_points_further_and_respects_the_radius`,
      `laplacian_smooth_relaxes_the_interior_and_can_hold_the_border`,
      `polygon_reduction_halves_a_triangulated_grid`,
      `wrap_bends_the_source_around_the_target_axis`,
      `new_modifiers_round_trip_and_default_their_optional_fields`,
      `wrap_modifier_uses_its_target_and_ignores_a_self_target`,
      `new_modifier_predicates_gate_formula_and_wrap`, plus the MCP add and
      rejection cases. Each modifier's manual page is not among the indexed
      sources, so its parameter set is a documented clean-room reading.

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
- [x] Clone tool. `clone_faces`/`clone_points` duplicate the selected polygons or
      points (empty selection means all) with the manual's Copies, Holes,
      Direction, interpolated Scale/Rotation around the selection centroid,
      position/scale/rotation variation and Keep ratio properties. Variations
      use a fixed `Pcg32` stream, so repeated Applies are deterministic; cloned
      faces keep corner UVs, material slots and interior creases, seams and
      crease weights. Mesh Editing has a **Clone** section and `Tools -> Clone`,
      enabled in point and polygon mode, and one undo restores the prior mesh.
      Tests: the four `clone_` geometry tests and the editor one-undo test.
- [x] Scalpel — one cut between two boundary picks of one polygon.
      `scalpel_face` + `ScalpelPick::{Point, Edge}` replace the polygon with the
      two faces on either side of the cut, insert a real point for an edge pick,
      split that edge in the neighbouring face too so the mesh stays watertight,
      carry a seam or crease weight onto both halves, keep corner UVs and the
      material slot, and refuse a cut that would leave fewer than three corners.
      Mesh Editing has a **Scalpel** button and `Tools -> Scalpel`, enabled in
      point and edge mode, where two selected edges of one polygon cut at their
      midpoints and two selected points cut between them; one undo restores the
      prior mesh.
      Tests: `scalpel_cuts_one_polygon_between_two_edges`,
      `scalpel_corner_to_edge_makes_a_triangle_and_a_quad`,
      `scalpel_splits_shared_edges_and_keeps_seams_and_creases`,
      `scalpel_rejects_picks_outside_the_polygon_and_degenerate_cuts` and
      `scalpel_cuts_between_two_edges_of_one_polygon_and_undo_restores_the_mesh`.
- [x] Scalpel chained click tool, as documented by source 43. `scalpel_face_ends`
      returns the two points a cut ends on, so the editor chains without
      guessing; the Mesh Editing **Scalpel** row's **Click tool** checkbox and
      `Tools -> Scalpel Tool` enable a viewport tool where a click on an edge or
      point starts a cut, a second click on an edge or point of a polygon that
      holds the start splits it (mixed edge/point ends allowed; a corner wins
      over the edge it sits on), the second click's point becomes the next
      start, and ESC resets the pending start without leaving the tool. A
      completed cut clears the stale component selection and is one
      `SetShapesCmd` undo step. The polygon under the pointer decides which face
      splits when two picks share an edge, and a pick that shares no polygon with
      the start is refused with the start kept, so a cut never chains across a
      polygon boundary; no middle click is involved. Undo, redo and a replaced
      document clear the pending start (it names points of the topology they
      replace), and a click whose pending start the current mesh no longer has
      is treated as a fresh start instead of being refused forever.
      Tests: `scalpel_face_ends_reports_the_points_a_chained_cut_continues_from`,
      `scalpel_click_tool_chains_cuts_and_escape_resets_the_start` (start → cut →
      chain → refused cross-polygon pick → ESC → undo, driven by real viewport
      clicks),
      `scalpel_click_tool_cuts_from_an_edge_to_a_point_and_chains_the_new_point`
      and `scalpel_pending_start_does_not_go_stale_across_undo_or_a_replaced_mesh`
      (undo clears the start, redo keeps it clear, an invalid start restarts the
      cut); each fails with the chaining, ESC or stale-start handling removed, and
      `scripts/check.sh` is green.
- [x] Offer a triangulation option for the split direction (shortest diagonal
      or quad pair) instead of always fanning from the first corner.
      `TriangulateMode::{ShortestDiagonal, QuadPair}` with
      `triangulate_faces_with`; shortest diagonal ear-clips choosing the shortest
      valid diagonal, quad pair splits a quad on its shorter diagonal, and the
      Mesh Editing **Split** selector chooses between them. The default is
      Shortest Diagonal, so the old fixed fan is gone.
      `triangulation_modes_choose_different_diagonals` covers a skewed quad, and
      the existing triangulate tests still check coverage, UVs, creases and the
      one-undo UI path.
- [x] Holes that enclose an island, and a click-in-viewport invocation instead of
      acting on the current component selection. `fill_holes` now labels mesh
      connected components and refuses a target border whose planar interior
      contains a point from another component
      (`fill_holes_refuses_a_hole_that_encloses_an_island`); the public manual
      does not define this case, so refusing with a clear status is the
      clean-room choice rather than covering the island with an n-gon. Mesh
      Editing's **Click tool** checkbox and `Tools -> Fill Hole Tool` pick the
      nearest open-border point on a viewport click and call the same
      `fill_holes`, staying active across holes; the click test covers filling
      a box's missing face from a screen click, one undo, and redo-safe depth.
      The selection-driven **Fill Hole** command still works in point, edge and
      polygon mode.

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
- [x] Attach to a running editor window so an agent can manipulate the live
      session and its selection instead of a separate in-memory document.
      Implemented 2026-11-24: `lc-mcp/src/bridge.rs` — loopback HTTP/JSON at
      `POST /command`, a 32-byte token from `/dev/urandom` required as
      `Authorization: Bearer`, an atomic `0600` registry (name, port, pid,
      token; `$XDG_RUNTIME_DIR` or `LEOPARDCRAFT_BRIDGE_REGISTRY`), a
      loopback-only `Origin` allow-list, `503` when the UI thread does not answer
      within three seconds, and a `LiveClient` with registry or
      `LEOPARDCRAFT_EDITOR_ENDPOINT`/`LEOPARDCRAFT_EDITOR_TOKEN` discovery that
      refuses to guess between several open editors; `lc-ui/src/live.rs` and the
      `Tools -> Live Agent Bridge` toggle service one request per frame on the UI
      thread and run each tool through the shared `McpServer` implementation on
      the editor's own document and history (one undo entry per call); `--attach`
      on `--mcp`/`--mcp-http` routes tool calls, `undo`/`redo`, the scene
      resources and `select_objects` to the editor and adds a `selection` field
      to `get_scene`, with the server's private document as the fallback when not
      attached. Tests: `attach_routes_tools_resources_and_undo_to_the_live_editor`,
      `bridge_answers_busy_when_the_editor_does_not_drain`,
      `bridge_requires_the_session_token_and_reports_the_editor_busy`,
      `registry_file_is_private_and_prunes_replaced_entries`,
      `origin_allow_list_accepts_loopback_origins_only`,
      `editor_host_services_bridge_requests_over_the_loopback_endpoint`, plus the
      three other `crates/lc-ui/src/live.rs` host tests.
      Verified 2026-11-24 against the running editor with the toggle on (registry
      entry `LeopardCraft`, port 39247, live pid): the real
      `leopardcraft --mcp --attach` session reported `attached to the live editor
      session`; `get_scene` returned the live 13-object document with
      `selection: {"names": ["Torus"], "objects": [11], "primary": 11}` — the
      editor's own selection, not a copy; `select_objects {"objects":
      ["Ground"], "mode": "replace"}` returned `{"names": ["Ground"], "objects":
      [8], "primary": 8}`; and a fresh, independent attach read `Ground` back
      from `get_scene`, proving the editor's selection had changed. The original
      `Torus` selection was restored afterwards.
- [x] Modifier-stack authoring tools: `add_modifier`, `update_modifier`,
      `remove_modifier` and `move_modifier` accept the same snake_case modifier
      objects `get_object` returns, validate the target object, finite
      parameters, Boolean operand and stack indices before applying anything,
      and record exactly one `SetShapeCmd` undo step per call. `add_modifier`
      appends or inserts, `update_modifier` replaces one entry, `remove_modifier`
      returns the removed object and `move_modifier` reorders. Tests cover
      insert/update/reorder/remove, undo/redo, a camera target, a self-referential
      Boolean, empty-stack updates and out-of-range indices with no undo entry.
      The tool inventory is updated in `AGENTS.md`, `README.md` and the MCP
      smoke-test list.
- [x] Material-graph authoring tools: `get_material` returns the complete
      material and shader graph, `create_material` adds a Principled material
      with optional name/base colour/roughness/metallic/flags/graph, and
      `update_material` changes its name, flags and complete graph. Graph input
      is validated by `ShaderGraph::validate` (unique node ids, one output,
      existing link endpoints, valid link inputs, unique link targets, finite
      positions/inputs/parameters and an advancing `next_id`) before any edit;
      each successful call is one `AddMaterialCmd`/`SetMaterialCmd` undo step.
      Material names are unique case-insensitively. Tests cover node addition,
      flags, create/rename, undo/redo, malformed graphs with no undo, duplicate
      names and native save/open round-trip.
- [x] Animation authoring tools: `get_animation` returns the active take, the
      take list and an object's complete clip; `set_animation_key` and
      `remove_animation_key` address translation/scale/FOV/light/morph channels
      and quaternion rotation keys using the existing `AnimChannel` registry and
      `ChannelAddress` validation; `create_take`, `activate_take`,
      `update_take` and `delete_take` wrap the document's existing take methods.
      Invalid object/channel combinations, component or morph-target indices,
      negative/non-finite times and no-op edits are refused before a command;
      each successful call is one `SetAnimationCmd` or `SetTakesCmd` undo step.
      Tests cover scalar and quaternion keys, removal, undo/redo, take
      create/activate/rename/fps/delete, malformed addressing with unchanged
      undo depth, and native save/open of takes and clips.
- [x] Mesh/UV/component authoring tools: `get_mesh` returns the editable mesh,
      topology, UV sets/slots/charts, seams and pins; `edit_mesh` transforms
      explicit point/face selections and runs extrude, inset, bevel, flip,
      triangulate, optimize, slide, fill_holes, create_polygon, disconnect,
      cover, collapse and bridge; `edit_uv` projects, unwraps, relaxes,
      normalizes, adds/removes sets, edits coordinates/pins/seams, packs and
      transforms charts and welds/splits/aligns/distributes/flips/rotates slots.
      Selection is per call (omitted face/point selection means all; omitted or
      empty chart selection means all charts), because a headless MCP document
      has no persistent component selection. Primitive meshes convert to
      editable inside the same `SetShapeCmd`, validation precedes the command,
      and each successful call is one undo entry. Tests cover single/multiple
      components, empty meshes and UV layers, seams/charts/pins, undo/redo,
      rejection depth, and native save/open of topology and UV sets.
- [x] Streamable HTTP transport, resource subscriptions and prompt templates.
      `leopardcraft --mcp-http [--addr <host:port>]` serves the protocol through
      `crates/lc-mcp/src/http.rs`: `POST /mcp` answers a request with JSON and a
      notification with `202 Accepted`, `GET /mcp` with
      `Accept: text/event-stream` opens the notification stream, and the
      listener refuses any non-loopback address (`127.0.0.1:8765` default), so
      the offline stdio path stays the default and HTTP is opt-in.
      `resources/subscribe`/`resources/unsubscribe` accept the two published
      resource URIs and every request that changes the document signature
      queues `notifications/resources/updated` per subscription; stdio writes
      them as extra protocol lines and the HTTP transport broadcasts them to the
      open streams. `prompts/list` publishes `scene_overview`, `object_report`
      and `render_check`, and `prompts/get` fills them from the live document
      with the same id-or-name object lookup the tools use; unknown URIs,
      prompts and arguments are invalid params.
      Tests: `http_transport_serves_requests_and_refuses_bad_ones`,
      `http_stream_delivers_resource_updates_after_edits`,
      `http_transport_refuses_non_loopback_addresses`,
      `resource_subscriptions_track_document_changes`,
      `prompt_templates_fill_from_the_live_document`,
      `stdio_serve_writes_subscription_notifications` and the real-process
      `http_transport_serves_the_protocol_from_the_real_binary`.
      Open: attaching to a running editor window (below) is not implemented.
- [x] Streamable HTTP sessions: the `initialize` response carries an
      `Mcp-Session-Id`, every later request has to repeat it (a missing header
      is `400`, an unknown or terminated id is `404` so the client initializes
      again), `DELETE /mcp` ends a session and drops its event streams, and at
      most 64 sessions stay live, with the oldest evicted first. Reconnecting a
      `GET` stream with a live id resumes the document's resource subscriptions,
      which live in the server rather than in the connection. Ids are a
      monotonic counter (`lc-…`) instead of the specification's
      cryptographically secure value, a documented local-only approximation; the
      shared document still serves every session, so the id controls resumption
      and not document isolation.
      Tests: `http_sessions_require_the_header_and_can_be_terminated`,
      `http_sessions_evict_the_oldest_at_the_cap` and the real-process
      `http_transport_serves_the_protocol_from_the_real_binary`, which now also
      proves a header-less request is refused.

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
- [x] Mirror the six lanes for the Codebuff CLI in `.agents/`, with an
      `lc-coordinator` that dispatches them and a shared, overridable model
      constant. (Freebuff loads the same files but refuses custom agent turns;
      see `docs/agents/spawning.md`.)
- [x] Document the `.agents` trust gate, Projects access, model selection and
      CLI usage in `.agents/README.md`.
- [x] Add `scripts/spawn-lane.sh [--print-packet] <lane> <packet|@file|->` as
      the single lane-dispatch interface, so a caller never branches on the
      host: it runs the host's `spawn_agents` bridge
      (`LEOPARDCRAFT_SPAWN_AGENTS`, agent id and packet as arguments with the
      packet on stdin) when one is exported, otherwise the promptable Codebuff
      CLI (`LEOPARDCRAFT_LANE_CLI`, default `codebuff`, invoked as
      `--trust-agents --cwd <repo> <prompt>`), and exits 2 with the one
      unblocking action when neither exists. `--print-packet` composes the
      prompt without dispatching for the interactive client.
      Tests: `spawn_agents_bridge_runs_first_and_reports_its_path`,
      `a_missing_bridge_falls_back_to_the_promptable_cli`,
      `a_failing_bridge_selects_the_cli_and_says_why`,
      `the_cli_failure_is_reported_instead_of_a_success_line`,
      `with_no_bridge_and_no_cli_the_error_names_the_one_action` and
      `packets_come_from_arguments_files_or_stdin_and_bad_input_is_refused` in
      `app/tests/lane_dispatch.rs`.
      Open: neither path runs in this environment (re-diagnosed 2026-11-24). The
      in-session `spawn_agents` tool refused the repository's lane agents and the
      built-in `basher` with the same `Agent "..." is not available to spawn`,
      because it advertises only `researcher-web`: the Desktop runtime registers
      its own built-in agents, never repository `.agents` definitions, and the
      `freebuff` binary here is the desktop bundle whose CLI accepts no prompt.
      `codebuff` is not installed, and `claude` 2.1.327 is installed with the
      project subagents in `.claude/agents/` but is not logged in, so it cannot
      dispatch either; `LEOPARDCRAFT_SPAWN_AGENTS` is unset. Unblock with
      `npm install -g codebuff` plus `--trust-agents`, a `claude /login`, or a
      bridge exported from a host that really can spawn. Per-host commands and
      the recurrence checklist are in `docs/agents/spawning.md`.
      Compatibility, checked against upstream source on 2026-11-24: Freebuff
      cannot run these lanes even though it loads `.agents`. Its parser takes no
      prompt or `--agent` (`cli/src/cli-args.ts`), and free-mode admission allows
      only bundled agent ids (`FREE_MODE_AGENT_MODELS`,
      `isFreeModeAllowedAgentModel` in `common/src/constants/free-agents.ts`), so
      `lc-*` turns are refused with `free_mode_invalid_agent_model`. The model
      string is not the issue: `deepseek/deepseek-v4-flash` is Freebuff's Flash
      wire id and local loading only requires a non-empty `id` and `model`. The
      blocker is the agent allowlist, so the working lane host is Codebuff
      (credits/API key) or Claude Code — Freebuff hours cannot fund lane agents.
- [x] Approve the project-scoped server in an interactive Claude Code session
      (user consent), then confirm `/mcp` lists the LeopardCraft tools.
      Confirmed by the user on 2026-11-24: Claude Code detects the project-scoped
      server and lists the LeopardCraft tools. Preconditions verified before the
      approval: `claude`
      2.1.327 is installed; `claude mcp list` in the repository root resolves the
      project-scoped entry to `${CLAUDE_PROJECT_DIR}/scripts/mcp-server.sh` and
      reports ⏸ Pending approval (run `claude` to approve), which is the
      expected pre-approval state; and the launcher answers the protocol
      directly (two response lines, empty stderr, 34 tools including
      `select_objects`, instructions naming `--attach`). The approval itself is
      per-user client state; the real-process smoke test
      (`app/tests/mcp_smoke.rs`) covers the launch and handshake the approval
      gates.

## Suggested order

1. **Animation Core** — continue with G33 channel addresses, grouped channels
   and scratch sampling; document takes, per-take fps, the channel model,
   quaternion rotation, manual tangents and selected-key/take retiming already
   cover interpolation, morph animation, takes and timing changes.
2. **Character rigging** and **UV Workflow** — weight authoring, tip-origin IK
   with take baking and clean-room joint limits are in place; viewport joint
   picking, spline IK, Cheetah3D-style constraint tags and UV tools remain
   user-visible gaps.
3. **glTF import and interpolation parity** — export covers skins, morphs and
   animation, native morph-weight F-curve rows are keyable, and import now reads
   skins plus STEP/LINEAR/CUBICSPLINE animation into takes. The remaining
   approximation is that cubic quaternion rotation imports as Smooth keys, and
   exact Cheetah3D linear/smooth/spline and weight-range semantics are not
   reproduced.
