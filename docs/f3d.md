# Native F3D investigation

Status: bounded archive inspection and **editable import of verified layout-8
circle/line/arc sketches, offset construction planes and one-sided-extrusion history**. General F3D history support remains
incomplete; the three larger supplied designs and the additional knob/clamp archives
do not import yet.
The project owner authorized clean-room investigation on 2026-10-11 as an
exception to AGENTS.md's public-specification requirement. Only the supplied
files and locally generated controlled variants were observed through Fusion's
public API; no proprietary or GPL/LGPL/AGPL implementation was used.

`file.inspect_f3d {"path":"...f3d"}` validates the archive and checks the native
object index against its bulk records, leaving the current design untouched:

```sh
cargo run --profile ci -p solvecraft-cli -- exec file.inspect_f3d '{"path":"/path/to/part.f3d"}'
```

`doc.open` reconstructs supported sketches, constraints, offset planes and extrusions and
regenerates the model before replacing the current document. Unsupported schemas
or regeneration failures leave the current document/model/undo state intact.
An imported document is unsaved with no source path: saving it uses a new
`.solvecraft` file, never JSON written over the original F3D. The desktop open
dialog and browser fallback picker include F3D, and the CLI supports opening,
measuring and benchmarking it. Browser file pickers reject oversized F3D files
before allocating their buffers; engine reads enforce the same archive limit.

Supported reconstruction includes native parameter names and expressions,
numeric user parameters, user/model roles, comments and favorites,
driving radius/diameter, angular, horizontal/vertical and line-offset distance dimensions, coincidence,
horizontal/vertical line and point alignment, parallel/perpendicular/collinear lines,
equal lengths/radii, point/line/circle symmetry, concentric/tangent round curves, point-on-curve and point/line-to-line-midpoint
relationships, independent point/curve fixation, construction curves, rigid root sketch placement, feature names/order/suppression,
complete outer and inner boundaries for circle, polygon and arc profiles, signed extrusion distances and taper. New-body
operations and booleans against a sole existing body are supported. Offset planes
retain named construction-plane and sketch dependencies for verified root frames,
including chained and signed offsets. Multiple-body
boolean targeting, two-sided/to-object extents, nonzero taper with start offsets, other sketch geometry and
constraint schemas, assembly transforms and other feature types fail explicitly.

## Evidence

| Supplied archive | Layout | Classes | Objects | Secondary records |
|---|---:|---:|---:|---:|
| adapter-x20-makita.f3d | 4 | 213 | 5,538 | 849 |
| R36S Grip.f3d | 5 | 207 | 3,952 | 629 |
| Gridfinity Storage Box Shelves (Parametric) V1.0.f3d | 5 | 223 | 4,899 | 784 |
| Minimal.f3d | 8 | 176 | 259 | 36 |

All contain `Manifest.dat` and paired `MetaStream.dat` and `BulkStream.dat`
parts beneath `Design1` (layouts 4/5) or `FusionDesignSegmentType1` (layout 8).
Minimal uses ZIP method 93 (Zstandard). The pure-Rust decoder checks frame window
and content bounds before allocation, limits expanded output, checks ZIP CRC32,
and rejects trailing frames. Separate browser/activity streams are not design
streams. Native B-rep geometry lives in
`.smb`/`.smbh` parts, not STEP. Previews and appearance payloads are not read or
extracted. None of the Fusion-produced files is copied into this repository.

Observed metadata layout (little-endian integers):

1. Length-prefixed UTF-8 `Design`, an empty string, and a length-prefixed UTF-16
   identity; two u32 values (build tag and layout), then `Design`, 0, 1, 0.
2. A u32-counted class table: identity string, parent string, u32 version, group
   string, and a u32-counted array of u64 object IDs for each class.
3. A u32-counted array of u64 root IDs.
4. A u32-counted primary index of `(u64 object ID, u64 bulk offset)` pairs.
5. A secondary index with the same pair layout.
6. An uninterpreted trailer: 8 bytes in observed layout 4, 12 in layout 5.

Layout 8 uses `FusionDesignSegmentType` in place of `Design`, adds two u32 header
fields after the layout, and uses the namespace string `Fusion`. Its trailer is
a u64 next identity, a zero u32, and counted `(UTF-8 name, u32 version)` pairs.
The class, root and record-index structures are otherwise unchanged in the
observed files.

Each indexed bulk record starts with a length-prefixed decimal tag and matching
u64 object identity. Decimal tags vary across files and are not stable feature
types. Class order, object ID order and index order do not establish timeline
order. Secondary records can lie between successive primary offsets, so those
intervals are not primary payload lengths.

The Rust reader retains both streams verbatim. It caps sizes, counts and strings;
rejects missing streams, unsafe/duplicate archive paths, duplicate identities,
out-of-range offsets and unsupported layouts; and validates identities in both
record tables. Index validity does not certify feature semantics.

## Verified parameter and circle records

In layout 8, parameter class `7A6A3D31-BE74-4E19-9115-20642944C2E4`, version 9,
contains UTF-16 expressions, names, labels and display units, plus numeric values.
Bindings (`D91D429C-1BB5-4640-B49F-69AF14F463E9`, version 0) connect model parameters to
their owning objects; user parameters have a null binding. Both binding back-references and duplicate numeric values
are checked. Numeric length values are **centimetres**, angles are **radians**;
`display_unit` is separate. The inspector exposes these units explicitly.

The supplied Minimal model has d1 = `100 mm` (native 10), d2 = `100.00 mm`
(native 10), and d3 = `0.0 deg` (native 0). d1 belongs to the sketch; d2/d3
belong to the extrusion. Controlled exports independently check:

| Edit | API/native observation |
|---|---|
| Height 125 mm | d2 becomes `125 mm`, value 12.5 |
| Diameter 80 mm | d1 becomes `80 mm`, value 8; circle radius becomes 4 |
| Restore original | All three original expressions and values restored |
| Reverse extrusion | d2 becomes `-( 100 mm )`, value -10; body extends downward |
| Restore positive extent | d2 becomes `100 mm`, value 10 |
| Rename sketch/extrusion | Parameter ownership and circle geometry retained |
| New offset circle on XZ | Local center (2, 3, 0), radius 4; world placement differs |

Full-circle class `F0130424-8B7E-4092-93C9-1CA807482534`, version 0, stores
center, normal, x-axis, radius and angle limits after counted integer attributes.
Its center-point and owning sketch references are validated by class/version.
The inspector reports **sketch-local centimetres**, not world coordinates.
The construction flag is verified by toggling it independently. Native profile
tags are retained separately from object IDs. Remaining presentation flags stay
opaque; these records alone are not a constraint-preserving sketch. Partial arcs
and unsupported payloads fail explicitly.

The original input plus eight API-exported cases cover these initial observations. The
local probe imports a temporary copy, exports locally, records API measurements,
and closes without saving the source cloud document. No Fusion installation is
needed to run the Rust inspector. Probe outputs remain outside Git.

## Timeline, placement and relationship controls

Nine further exports independently vary suppression, a second sketch/extrusion,
sketch reordering and new-body/join/cut/intersect operations. Another eight exports
cover two-sided and tapered extents, construction circles, and deletion of the
diameter and coincidence constraints. All 25 controls remain local.

* Timeline class `2F4C1849-1A5A-4F6C-A086-8DD445CBF94B`, version 3, contains the
  ordered feature references. The reorder control changes this list while leaving
  the registration order and evaluation dependency graph unchanged. Duplicate,
  missing or unsupported timeline entries fail; they are never silently dropped.
* Sketch feature class `8DA771B7-52ED-42FC-94E2-2353FE141373`, version 5, links to
  geometry and placement. Extrude class `DD405BC2-D673-44F0-8833-5CB2A1C186C7`,
  version 8, identifies body operations and profile containers. Native operation
  values 1/2/3/4 correspond to join/cut/intersect/new body. Four separate reference
  lists after the profile containers vary with boolean operations; their roles
  are still opaque. Two-sided extent layouts are explicitly unsupported.
* Shared feature metadata supplies names, dependencies and two independently
  checked suppression flags. Sketch custom names live in the geometry's secondary
  record. Cached evaluation counters do not establish feature order or suppression.
* Transform class `F47A46FB-DA27-4AFA-8A72-1B38FA596E23`, version 0, has compact
  identity and explicit 16-double encodings. The reported row-major local-to-world
  matrix matches the public API, including the offset-circle XZ example. Translation
  units remain centimetres. Assembly placement is not reconstructed.
* Selected circular profiles follow container → profile → tracking record →
  `(primary, secondary)` curve tags, scoped to the referenced sketch. A curve's
  object ID is not its profile tag. Verified regions contain finite lines, arcs
  and circles, including inner loops and intersected analytic pieces (see the
  split-boundary and rotated-seam controls below). Ownership and back-references
  are checked, rather than selecting the first circle in a sketch.
* Generic constraint class `60403D47-0C49-49B0-BDE8-1679608164A2`, version 3, uses
  kind 1 for coincidence between two points. The importer checks kind and operands;
  a class ID alone is insufficient (horizontal/vertical controls share it).
  Diameter class `664AD49F-207F-4CD5-8555-DD269E9F09EA`, version 0, connects
  a circle to a parameter binding. The decoder checks owning sketches and repeated
  references. Constraint-removal controls independently verify both lists.

The inspector reports `history`, `circle_profiles` and `sketch_constraints`, each
with either supported records or an explicit unsupported error. Unknown fields
remain retained in the raw streams, not interpreted as defaults.

## Remaining history work

Full extent/start/direction schemas, other sketch geometry and constraints,
free-form partial-curve profiles, assemblies and native B-reps remain unresolved. Parameter
expressions are preserved and checked against the native cached values; unsupported
units or expressions fail instead of becoming constants. Native mm/degree input
parameters now bind directly to feature inputs, so timeline edits update dependent
native expressions. Other display units retain their unit-bearing stored parameter
and generated feature-input alias until per-input unit contexts are supported.

Point class `C2CEDAE7-1716-47C1-B7B1-07B70081D0FB` version 11 stores local coordinates,
fixation and origin identity. Five independent point/circle fixation controls
establish that fixed state is distinct from derived constraint state. Treating the
latter as a Fix would make the supplied diameter dimension uneditable. The origin
maps to SolveCraft's existing fixed origin instead of creating a duplicate point.
Taper volume measurements establish the same positive sign in both applications.

Independent API inspection of the three larger designs found loft, fillet,
chamfer, mirror, pattern, sweep, combine, construction, draft, offset-face,
delete-face, remove and base features in addition to sketches/extrusions. Those
feature types and legacy record versions still require reconstruction and tests.
General history preservation is not implemented.
Opaque-byte retention alone must not be called editable import.

Autodesk's public [ImportManager](https://help.autodesk.com/cloudhelp/ENU/Fusion-360-API/files/core_ImportManager.htm)
and [TimelineObject](https://help.autodesk.com/cloudhelp/ENU/Fusion-360-API/files/fusion_TimelineObject.htm)
APIs offer an independent validation route inside Fusion. They are not native
stream specifications; the inspector itself does not require Fusion.

## Validation

Original synthetic tests cover all three layouts, opaque-byte retention, secondary
records, truncated/malformed indexes, unsafe packages, Zstandard CRC/size limits,
UTF-16 parameter fields, invalid circle geometry, actual timeline ordering,
construction, profile selection, constraint links and truncated relationship records. The engine test checks
that failed opening preserves the current model and undo history. The local
corpus test checks all four files, expected counts, a five-second budget, and
unchanged document state:

```sh
SOLVECRAFT_F3D_CORPUS=/path/to/test_files cargo test --profile ci -p solvecraft-cli --test f3d_inspection supplied_f3d_indexes -- --ignored --nocapture
SOLVECRAFT_F3D_PROBES=/path/to/probes cargo test --profile ci -p solvecraft-cli --test f3d_inspection controlled_parameter_edits_match_fusion_api -- --ignored
SOLVECRAFT_F3D_PROBES=/path/to/probes cargo test --profile ci -p solvecraft-cli --test f3d_inspection controlled_history_matches_fusion_api -- --ignored
SOLVECRAFT_F3D_PROBES=/path/to/probes cargo test --profile ci -p solvecraft-cli --test f3d_inspection controlled_construction_and_constraint_removal -- --ignored
SOLVECRAFT_F3D_PROBES=/path/to/probes SOLVECRAFT_F3D_CORPUS=/path/to/test_files cargo test --profile ci -p solvecraft-cli --test f3d_import -- --ignored
```

Observed inspection times were under 8 ms per archive. The controlled-variant
test compares expressions, units, values, circle centers and radii directly to
the separately recorded Fusion API measurements. Actual Fusion files remain
outside the repository; ordinary CI uses original synthetic records.

Twenty-seven [small editable SolveCraft fixtures](../apps/solvecraft-cli/tests/history/README.md)
provide command recipes, saved documents and independent analytic volume/bounds
checks. They exercise parameter edits, signed heights, placement, all four body
operations, reordering, suppression, reopening and undo. These native fixtures run
in ordinary CI without Fusion; they do not claim successful F3D reconstruction.

Full `cargo xtask ci` passed with upstream `a094fa4` integrated, including all 24
test binaries, the opening-performance guard and the ten-crate WASM check. All
four local evidence tests passed on that same build: four supplied archives and
25 controlled exports. This validates inspection and the separate native history
fixtures, not editable F3D import. Subsequent importer checks cover the original
Minimal archive, 24 supported history/extent controls and five fixation controls:
30 successful native imports in total. Volumes are compared with independent
Fusion API measurements at 0.1% tolerance; available world bounds use 0.00001 mm.
The two-sided control explicitly checks atomic rejection. Additional tests edit
the imported diameter/height and a dependent expression, then check volume and
undo. Original synthetic records in ordinary CI reconstruct an editable document,
check analytic volume before/after edits and reopening, and exercise suppression,
fixation, unknown versions and every truncated point prefix. No Fusion-generated
archive or geometry is committed.

The importer milestone passes `cargo xtask ci` against upstream `a094fa4`:
25 test binaries, including 29 IO tests and the original editable fixtures,
plus the ten-crate WASM check. The HardEdgeBasin performance test completed in
0.87 seconds on this run. Local import and inspection evidence tests also passed
using the workspace build. Rectangular-profile controls have been generated for
the next stage but are not supported by this milestone.

Upstream subsequently advanced to `5affff3` (0.2.0, PR #150). The integration
preserves the published branch history with a merge; the generated license
manifest was regenerated to resolve its version conflict. Full CI passed again:
25 binaries, 187 kernel tests, 549 engine tests, 292 UI tests, the ten-crate WASM
check and the opening guard (2.65 seconds for the performance test). All eight
local import/inspection tests passed against this integrated build as well.

## Finite lines and rectangular history

Three independent rectangle exports vary width and height. Line class
`DCA267ED-D615-4934-B64F-AD805E8003E2`, version 2, stores a start, delta,
normalized direction and endpoint references (end first, then start). Imported
endpoints must agree with the independently decoded point records. Five isolated
line controls establish fixation and construction flags separately.

The generic geometric relationship class uses kind 64 for horizontal and 128 for
vertical, each with one line operand. Linear dimension class
`8C780195-72C0-4a56-A911-E43AB14357F2`, version 3, links two points to a
driving parameter, with separate horizontal and vertical orientations. Profile
tracking resolves every curve tag in its original selected loop, rather than
relying on geometry registration order. Synthetic tests intentionally reverse it.

Six diameter/linear dimension controls toggle
[isDriving](https://help.autodesk.com/cloudhelp/ENU/Fusion-360-API/files/fusion_SketchLinearDimension_isDriving.htm).
Driven dimensions change parameter expression metadata and remain explicitly
unsupported: importing them as driving constants would lose their semantics.
Both actual-file atomicity checks and an original synthetic metadata test guard
that behavior. These observations do not establish support for other dimension
orientations or attached sketches.

The two new original rectangular-prism fixtures use 30 × 20 × 17 mm dimensions
and analytic 10,200 mm³ volume, including negative height. They test width/depth/
height edits, a dependent expression, world bounds, serialization, reopening and
undo without requiring Fusion. Local rectangular imports additionally compare
volumes to independent API measurements and exercise native parameter edits.

This rectangle milestone passes full `cargo xtask ci` with upstream `2f78d3e`
(merged PR #149) integrated without conflicts: 25 test binaries, 31 IO tests,
552 engine tests, 187 kernel tests, the eleven native fixtures, and ten-crate
WASM validation. The opening-performance test completed in 1.97 seconds.
All eleven local evidence tests passed (seven import, four inspection), including
38 supported original/rectangle/line imports plus four driving-dimension controls
and atomic rejection of the two driven-dimension controls. The imported rectangle
was also rendered and visually checked; local generated evidence stays outside Git.

## Native feature input identities

For mm/degree inputs, an extrusion's distance and taper use their native names
directly in SolveCraft's feature parameter table. The original expressions live
on the feature, and the duplicate stored parameters are removed. Parameter edits
and timeline edits therefore affect the same definition. The local rectangle
test makes width depend on the native extrusion height, edits the extrusion via
`timeline.edit`, and checks volume, parameter rows, reopening and undo. An original
synthetic test checks the same dependency and absence of duplicate definitions.

Non-default units keep the earlier unit-bearing indirection so that bare numbers
retain their meaning (for example, `1.7` cm remains 17 mm). A synthetic regression
checks that behavior and rejects dimensionally incorrect extrusion bindings.

The input-identity follow-up passes full CI against the same upstream `2f78d3e`:
33 IO tests, all 25 test binaries and the ten-crate WASM check. The opening guard
completed in 0.91 seconds. All seven local import tests pass, including timeline
editing with a dependent native sketch dimension.

## Complete profiles with holes and arcs

The tracking record contains a schema tag, counted loops, counted curve entries
per loop, one outer/inner flag per loop, then a zero trailer. Curve entries retain
orientation separately from boundary role. Five controls cover an annulus with
independent inner/outer diameter changes, two disjoint holes, and a rectangular
outer loop with a circular hole. Exactly one outer loop and disjoint complete
curve identities are required. The importer uses SolveCraft's `WholeBoundaries`
selection, with canonical outer/hole IDs, so missing holes cannot become solid disks.

Circular geometry class `F0130424-8B7E-4092-93C9-1CA807482534` version 0 also
represents arcs. Arcs retain angle limits and center/end/start point references,
followed by a 42-byte flag/presentation section (full circles use 44 bytes).
Independent fixation/construction controls locate the arc flags; their offsets
must not be borrowed from the full-circle layout. Endpoint positions are checked
against the native center, radius, normal, local X axis and angle limits before
constructing a shared-point SolveCraft arc. Quarter, half, major, rotated negative
sweep and changed-radius controls check volume and actual API endpoints.
Fusion's documented
[arc creation behavior](https://help.autodesk.com/cloudhelp/ENU/Fusion-360-API/files/fusion_SketchArcs_addByCenterStartSweep.htm)
makes arcs counterclockwise even for negative requested sweeps; the native local
axis changes in that control. Discarding that axis would mirror the result.

The annulus controls additionally verify diameter operand axes 4 and 5; both
remain diameter relationships. Other axis encodings are rejected. Original
synthetic tests check analytic volumes, native edits/reopening, shared arc/chord
points, missing-hole failures, malformed loops and every truncated arc prefix.
Two original native fixtures add annulus and semicircular-segment volume,
bounds, dimensional edits and undo checks. General history import remains
incomplete; these changes do not establish support for the larger supplied files.

This milestone passes full `cargo xtask ci`: 37 IO tests, all 25 test binaries,
thirteen native history fixtures and ten-crate WASM validation. The opening guard
completed in 0.87 seconds. All thirteen local evidence groups pass (nine import,
four inspection); the main import group checks 42 supported controlled archives
against independent API volumes and reopening. Annulus, major-arc and negative-sweep
imports were rendered and visually checked; generated evidence remains local.

Upstream `7abfe69` (merged documentation PR #157) was then integrated without
conflicts. Post-merge full CI also passes, including the opening guard in 0.91 seconds.

## Radius dimensions and geometric relationships

Nine independent radius/arc-diameter controls establish the circular dimension
schemas: radial class `8EDAC24D-35D4-4591-B8E7-B3EA081C9DF0` version 0, diameter
class `664AD49F-207F-4CD5-8555-DD269E9F09EA` version 0. The trailing dimension flag
is respectively `0x20000` or `0x10000`, and must agree with the class. Both bind
native parameters to SolveCraft dimensions on circles or arcs. The importer checks
length units. Reference dimensions still fail explicitly until their measurement
dependencies can be represented, rather than becoming frozen driving parameters.

Thirty-three geometric controls add/remove eleven relationships independently.
The generic version-3 constraint record uses the following verified kind tags:
collinear `2`, concentric `4`, parallel `16`, perpendicular `32`, horizontal `64`,
vertical `128`, tangent `256`, equal `2048`, midpoint `4096`; coincidence and
point-on-curve share `1`. Operand count and classes distinguish point alignment
from line alignment and point coincidence from point-on-curve. The importer
checks operand types, sketch ownership and matching back-references; an unknown
combination is rejected. Arc midpoints are not mapped to chord midpoints.

Original synthetic prisms check analytic volume after edits to dimensions whose
geometry is held by parallel, perpendicular, equal and aligned-point constraints.
Local native controls check constraint identity, solver behavior after point edits,
radius/diameter edits, independent API volume, reopening and undo. Upstream file
read limits are also applied to F3D opening, inspection and CLI benchmarking.

Verified against upstream `d9d4dd2` (merged PRs #151–155): full CI passes all 25
test binaries, 46 IO tests, 553 engine tests, 191 kernel tests and ten-crate WASM
validation. The thirteen native fixtures pass; the opening guard completed in
0.85 seconds. All fifteen local evidence tests pass (eleven import, four
inspection). The edited arc extrusion was rendered and visually checked. The
larger supplied designs still require legacy schemas and additional feature types.

## Offset construction planes

Six independent API controls cover XY, XZ, negative offsets and two-plane chains.
The version-4 timeline class `D869265F-F339-4751-A066-91D30F81FF08` points to
plane geometry and a version-15 definition (`7B6C3C2D-4096-4E6D-B225-699E8C2D9355`).
The verified offset method is `2`. Its distance binding belongs to the plane
feature; its base follows a version-1 reference wrapper to a version-0 entity
tracker. A chain must terminate in a named plane belonging to the root origin.
Missing, forward, cyclic, foreign-owner and unsupported-method references fail.

The cached definition matrix validates reconstructed origin/normal, but does not
replace the dependency. Native surface axes can differ from the attached sketch
axes (observed on XZ). The supported origin frames use SolveCraft's matching
sketch axes and validate each attached sketch's full frame. Unverified relative
frames fail explicitly. Nonzero or expression-dependent internal `ExtraOffset`
slots are unsupported; the observed literal-zero slot is not a public Fusion
parameter and is excluded from the imported parameter table.

Two original field-authored tests cover chained plane edits, native input names,
reopening, invalid references and every truncated interpreted record. Three new
native `.solvecraft` fixtures bring the editable history set to sixteen. Their
radius-10 mm, height-15 mm cylinders have analytic volume 1500π mm³; bounds must
move after parent parameter and timeline edits, including negative offsets.
The shared fixture runner now checks bounds after edits, reopening and undo as
well as at initial opening. Local native controls independently check API volumes,
bounds and timeline names; the edited dependency tests cover XY, XZ and chains.

Validation: full `cargo xtask ci` passes all 25 binaries, 48 IO tests, sixteen
native fixtures and ten-crate WASM checks. The opening guard completed in 0.87
seconds. All sixteen local evidence tests pass (twelve import, four inspection),
including the six new plane archives. The native chain fixture and an imported
XZ cylinder after a negative offset edit were rendered and visually checked.
Browser picker changes compile for WASM; a browser UI import run remains to be
verified. General F3D history support is still incomplete.

## Legacy parameter schemas and editable user parameters

The supplied originals and their locally resaved copies contain 411 matching
parameter identities: adapter 190, R36S 70, Gridfinity 151. Their names, expressions,
labels, comments, units, bindings and cached values agree across the following
verified schema pairs. Other layout/version combinations fail explicitly.

| Archive layout | Parameter version | Flags before comment | Metadata zero prefix |
|---|---:|---:|---:|
| 4 | 5 | 3 bytes | 0 bytes |
| 5 | 6 | 7 bytes | 0 bytes |
| 5 | 7 | 8 bytes | 4 bytes |
| 8 | 9 | 12 bytes | 5 bytes |

The comment is a variable-length UTF-16 string before the parameter sequence and
nullable binding. The five-byte expression metadata suffix records driven,
favorite and user flags. A user parameter must have a null binding; a model
parameter must have a valid owning binding. The inspector exposes driven status,
but import still rejects driven parameters until their measurement dependencies
can be reconstructed. Text parameters remain unsupported.

Five circular and three rectangular controls were authored with the documented
[user-parameter API](https://help.autodesk.com/cloudhelp/ENU/Fusion-360-API/files/fusion_UserParameters_add.htm)
and [parameter properties](https://help.autodesk.com/cloudhelp/ENU/Fusion-360-API/files/fusion_Parameter.htm).
They vary comments (including Unicode/newlines), favorites, dependent expressions
and a unitless multiplier. Native dimensions show a variable UTF-16 `fx:` prefix
when their value is an expression; circular and linear decoders now read that
prefix instead of assuming a fixed presentation length.

Modern resaves of legacy archives can retain `Design` as the initial stream name
while using the layout-8 `FusionDesignSegmentType` descriptor. This observed
header combination is accepted and has an original synthetic regression test.

Original tests cover all verified parameter schemas, roles, flags, comments,
truncation, user-driven contradictions, unknown layout/version combinations,
dependency edits and reopening. A seventeenth native fixture has diameter `base`
and height `base * scale`; parameter edits must update its analytic volume and
bounds. Local evidence additionally checks parameter renaming, serialization,
undo, user/model classification and public-API measurements. This work exposes
legacy parameters; it does not yet import the larger archives' full histories.

Eleven legacy count parameters use the observed version-1 integer binding class
`A085449A-5144-4B2B-B455-7F7035A40559` instead of the double binding. These slots
must be unitless; ownership, parameter backreferences and cached values are
checked. Original tests cover invalid units, values, owners and truncation.

Validation: `cargo xtask ci` passes all 25 test binaries, 53 IO tests, seventeen
native fixtures and ten-crate WASM checks. The opening guard completed in 1.04
seconds. All eighteen local evidence groups pass (thirteen import and five
inspection), including the 411-record legacy comparison and eight new user
parameter controls. An imported rectangle after renaming and editing its base
parameter was rendered and visually checked.

## Aligned point dimensions

Twelve local rectangular controls use the documented
[point-distance API](https://help.autodesk.com/cloudhelp/ENU/Fusion-360-API/files/fusion_SketchDimensions_addDistanceDimension.htm)
with aligned orientation. They vary diagonal length, rectangle aspect ratio and
selected-point order. The observed version-3 linear dimension uses flag `0x4000`,
versus `0x800000` for vertical distance. It reconstructs as a Euclidean
point-distance constraint. Unknown kind flags still fail. The connection fields
originally interpreted as qualifier/axis were corrected by the experiments below.

Fusion normalizes the primary operand order but retains the selected order in
the final reference list. Linear dimensions permit either ordering of the same
two distinct points; mismatched membership, ownership and bindings fail.
Original field-authored tests check this interpretation, dependency edits,
reopening, invalid kind flags/references and every truncated record prefix.

An eighteenth native fixture defines a rectangle by width and diagonal, then
extrudes it. Its side length follows √(diagonal² − width²), with analytic volume
and bounds checked after edits, reopening and undo. Local import checks also
compare all twelve controls with independent public-API measurements and edit
their diagonal to a value absent from the exported controls.

Validation: full `cargo xtask ci` passes all 25 binaries, 55 IO tests, eighteen
native fixtures and ten-crate WASM checks; the opening regression guard passes
in 2.42 seconds. All fourteen local import groups pass, including the twelve
aligned controls. Both the authored fixture and a reversed-selection import
after a diagonal edit were rendered and visually checked. Upstream `a7512e5`
(browser-storage documentation) was merged without conflicts.

## Constraint connection metadata

Eight further controls vary constraint/dimension creation order, horizontal and
vertical selection order, and diameter dimensions on concentric circles. They
show that the u32 after each operand reference is graph connection metadata,
not an orientation or geometric subtype. Inserting the aligned dimension first
swaps its connection fields with the width dimension while retaining its kind
flag. A vertical dimension can have the same fields as a horizontal dimension.
Concentric circles have nonzero curve connection slots in their diameter records.

The reader consumes these opaque slots without using them as indexes or geometry.
Constraint semantics remain checked through the kind flag, explicit entities,
backreference membership, owning sketch and parameter binding. Both orderings
of a linear dimension's selected points are accepted because all three supported
distance kinds are unsigned. Unknown kinds, duplicate entities, foreign owners,
missing bindings and mismatched backreferences still fail explicitly.

Original tests vary connection slots through the full u32 range and verify
unchanged relationships and geometry. Local controls check API volumes/bounds,
dimension edits, serialization/reopening and undo. No Fusion export is committed.

Validation: all fifteen local import groups and thirty-six focused F3D tests pass.
Full CI passes all 25 binaries, 57 IO tests, eighteen fixtures and the WASM checks
with `RUST_TEST_THREADS=2`; the opening guard completes in 6.76 seconds under
concurrent machine load. The first default-parallel run failed only the unrelated
IGES fan-out wall-clock guard (13.76 seconds); the unchanged test passed alone in
2.39 seconds and passed in the complete retry. The edited concentric ring was
rendered and its retained hole visually checked.


### Optional metadata, centerlines and projection state

Modern exports of legacy sketches may omit the metadata dictionary entirely.
The shared reader accepts absent or present dictionaries, bounds entry counts and
strings, and rejects duplicate keys and unknown types. Original complete-circle
and rectangle records verify identical imported histories, dimensional edits,
analytic volumes and reopening with and without empty dictionaries.

Five local public-API controls project a line and circle into a second sketch,
break their links, then toggle the source line between centerline, construction
and normal. The point flag immediately after origin identifies linked points;
the first curve flag after fixation identifies reference curves. These flags
are exposed by the native reader. Import still rejects unresolved linked source
dependencies; cached positions are not a substitute for the dependency graph.
The construction byte distinguishes normal (0), construction (1) and centerline
(2). Imported centerlines retain SolveCraft's centerline property and stay out of
profiles. The original centerline-extrude fixture checks the unchanged prism,
width/depth/height edits, analytic volume, bounds, reopening and undo.

The larger-archive investigation additionally identified point/line subclasses
that split geometry between primary and secondary records, and variable curve
tails containing references. A local acceptance comparison remains pending for
these layouts. This milestone does not establish complete legacy sketch or
feature-history import. All Fusion controls remain outside Git.

Validation: all five projection/line-type controls and all fifteen local native
import groups pass. Full CI passes 25 binaries, 62 IO tests, nineteen original
fixtures, 553 engine tests and the WASM checks. The opening regression takes
0.89 seconds in this run. Upstream main through 9d7be3c is integrated, including
deterministic STEP/IGES work-budget tests. The centerline prism was rendered and
visually checked as one unsplit solid.


### Associative sketch-curve projections

The sketch feature's former twelve-byte neutral block contains a variable
projection-reference list. Supported projection records bind a target curve to
a source selector and a tracking identity. Both identities must agree on the
source sketch geometry and curve tag; the importer validates versions, owners,
member entities, unique tags, coordinate transforms and cached geometry.

Line and circle projections now become SolveCraft sketch links to an earlier
source feature. Points and curves retain native identities. Adjacent projected
lines can share a native corner: each SolveCraft link owns an internal point
alias, all aliases must resolve to the same native source vertex, and dimensions
and constraints use the canonical point. Independent cached points are never
substituted for a missing relationship. Body-edge/face projections, multi-entity
mappings and projected arcs still require additional native schemas.

Local controls cover a projected circle extrusion, diameter and height edits,
reopening, a second source edit after reopening and undo; a projected rectangle
covers four linked edges and shared source vertices. The original projected-circle
SolveCraft recipe and fixture add source-diameter edits with analytic volume and
bounds. Original synthetic records exercise reconstruction and reject damaged
ownership, target membership, missing/forward sources and truncated records.

Validation: all seventeen local F3D import groups pass, including four projected
circle exports and three projected rectangle exports. Source edits, reopening,
undo and edited-cylinder rendering were verified. Integrated upstream main
through 6f872c7 passes full CI: 26 binaries, 64 IO tests, twenty native fixtures,
553 engine tests and WASM. The opening guard completes in 0.91 seconds.


### Topology-operation ownership and persistent edge projections

Modern operation-map records (`5D89B935-3F04-4CAB-81F5-E7B309E7E13F`,
version 1) associate native topology tags with feature records in both directions.
`file.inspect_f3d` reports these as `operation_owners`. The bounded reader rejects
duplicate tags/features, missing objects, conflicting maps, unsupported metadata
and truncated payloads. Top/bottom circular-edge controls share a producing
extrusion; a second overlapping extrusion has different operation tags. The native
feature identities were checked against independently captured public API tokens.
Tags are not bulk-record IDs and no geometry matching is used for this inspection.
The additional populated list in legacy files and the version 0 map remain
unsupported; their meaning is not inferred from empty modern controls.

New SolveCraft edge projection links also retain a persistent edge name and resolve
that exact edge on the specified body. This prepares the document layer for native
body-edge references. Large radius/height edits cannot reselect a nearby edge or
an overlapping body. A missing or ambiguous name marks the link lost and keeps its
last geometry. Existing unnamed links keep their historical behavior. Clone
remapping preserves both feature and body identities.

The twenty-first original fixture, `projected-body-edge`, checks three overlapping
cylinders and a projection of the first cylinder's top edge. Analytic summed
volume, bounds, a fivefold diameter increase, height reduction, reopening and undo
are covered. Separate regression tests verify lost-reference handling even when
another body remains at the old coordinates. No Fusion output is committed.

F3D body-edge import itself remains unsupported: the native selector, body
metadata and complete edge-recipe interpretation still need verified decoding.
This operation map and persistent link support do not imply general F3D history
reconstruction.

Validation: upstream `ef8f6ca` is integrated. Full `cargo xtask ci` passed
27 test binaries, 66 I/O tests, 107 document tests, 558 engine tests, 198 kernel
tests, 299 UI tests, all 21 native fixtures and ten-crate WASM checking. The
opening-performance test passed in 0.86 seconds. All seventeen local F3D import
groups and the three body-reference operation-map controls passed separately.


### Editable circular body-edge projections

Native body-edge selectors now reconstruct the verified circular start/end cap
edges of positive, untapered, independent circular extrusions. The selector's
body handle names both the native body and its producing feature; the feature's
output list must point back to that handle. The independently decoded operation
map and mapping-side tracking recipe must agree with the selector. Source and
tracking identities have distinct verified final role markers. Unknown recipe
trees, multi-body outputs, mismatched references and unsupported producers fail
atomically. Cached native circles are validation data, never replacement solids.

The importer resolves the source profile's persistent side/cap identities on the
regenerated body. SolveCraft's two semicircular edges become one circular sketch
link, retaining the native curve and center IDs. Every recompute follows the same
named edges, joins them by endpoint incidence and requires one closed circular
projection. Lost members or changed shapes retain cached geometry with a lost-link
warning. Generic body edges, taper, negative source extents, booleans and changed
topology still need further native schemas and reference policies.

`sketch.project` also accepts `circle: true` with explicitly selected body edges.
Incomplete chains, duplicate edges, mixed bodies and noncircular projections are
rejected. The twenty-second original fixture (`projected-body-circle`) checks
three overlapping extrusions, one projected circle/link, analytic volume/bounds,
large source edits, reopening and undo. File format 14 preserves grouped circular
edge identities; existing links keep their previous semantics during migration.

Original field-authored records cover both cap roles, malformed/truncated
selectors, ownership, tracking and edits in regular CI. Seven local native exports
cover top/bottom cap selection, overlapping source bodies, diameter edits and target
height edits. Source diameter/height changes, edits after reopening and volume
checks also pass against those controls. All eighteen local F3D import groups pass.
General editable F3D import remains incomplete, including the larger supplied
feature histories. No Fusion-generated assets are committed.

Validation: upstream `1d30480` is integrated. Full `cargo xtask ci` passed
28 test binaries, including 68 I/O tests, 107 document tests, 559 engine tests,
198 kernel tests, 301 UI tests, all 22 native fixtures and ten-crate WASM checking.
The opening-performance test passed in 0.91 seconds. All eighteen local F3D import
groups passed; the actual imported overlapping-body control was rendered after
source diameter and height edits.


### Editable rectangular body-edge projections

Finite cap edges of positive, untapered, independent rectangular extrusions now
import as associative line links. Four-term edge recipes validate side/cap/adjacent
face labels, operation ownership, the complete recipe tree and tracking roles.
The finite cached line has independently checked tracking identity, finite geometry
and complete framing; it is never used as replacement sketch geometry.

Native profile ordering differs even between independently authored equivalent
rectangles. Side-face labels must not be mapped by that ordering. Instead, the
complete finite 3D source edge must match exactly one side of the explicitly owned
rectangular profile, at the verified cap height. Endpoint incidence and right angles
are checked, then the regenerated body's persistent edge and the native target
projection must agree. Ambiguous or inconsistent matches fail without a nearest-edge
fallback. Subsequent edits use persistent body/edge names. Adjacent projected curves
retain native curve IDs and canonical source-corner aliases.

Twelve local exports cover top/bottom edges, overlapping bodies and independent
width/depth/source-height changes. Import, subsequent parameter edits, reopening
and undo match public-API volumes. Original record tests cover all rectangular
recipe roles, truncated/malformed identities, finite-edge cache tracking and native
profile-order permutations. The twenty-third original fixture,
`projected-body-rectangle`, checks analytic volume and bounds for three overlapping
extrusions through large width/depth/height changes, reopening and undo.

This remains bounded rectangular support. General body-edge recipes, arbitrary
polygons, modified producers, tapered or negative source extrusions, and the larger
supplied feature histories remain unfinished. No Fusion-generated assets are committed.

Validation: upstream `1d30480` remains integrated. Full `cargo xtask ci` passed
all 28 test binaries, including 71 I/O tests, 107 document tests, 559 engine tests,
198 kernel tests, 301 UI tests, 23 original fixtures and ten-crate WASM checking.
The HardEdgeBasin opening regression passed in 0.90 seconds. All nineteen local
F3D import groups passed (the corpus-only Minimal check was run separately with
its required corpus path). The edited overlapping rectangular-body import was
rendered and visually checked.


### Legacy timelines and editable extrusion start offsets

The inspector now preserves every timeline position in supplied layout-4/5/8
archives, reporting unsupported entries individually while decoding later supported
entries. Full import remains atomic: partial inspection does not silently omit
features from a reconstructed design. Timeline versions 2/3, sketch features 3/4/5,
geometry placement 16/17/18 and inherited common-feature versions 1/2 have distinct,
validated layouts. The version-17 secondary entity map is retained explicitly;
reconstruction rejects it until its semantics are verified.

The three older designs contain 137 timeline entries in total. Inspection decodes
all seventeen timeline sketches, preserving original custom names, entity references,
transforms and suppression. Public API timeline order and counts provide independent
checks. Legacy geometry, constraints and unsupported feature types still prevent
complete editable imports of these designs.

One-sided distance extrusions now preserve the optional signed start-offset
parameter, including its native name, expression and owning feature. The extent
header contains separate one-byte start-kind and offset-presence fields; optional
bindings and framing are checked before reading profiles. Body-feature versions
1/2 have three/four reference-role lists respectively. Shared decoding keeps
inspection, reconstruction and producer validation consistent.

Three local exports independently test positive, negative and expression-based
start offsets. Height/diameter/offset edits, volumes, world bounds, reopening and
undo match public API measurements. Imported zero-angle taper parameters remain
editable while using ordinary extrusion evaluation; nonzero taper with a start
offset is still unsupported. Body-edge projections from offset extrusions are not
yet supported. Field-authored records reject truncated bindings and invalid flags.

The twenty-fourth original fixture, `extrusion-start-offset`, is a 30 × 20 × 17 mm
prism starting at `height / 2`. Its analytic volume is 10,200 mm³. Independent
negative-offset, height and width edits check volume and bounds, reopening and undo.
All twenty local native-import test groups and all twenty-four original fixtures
pass. No Fusion-produced files are committed.

Validation: upstream `af536a7` is integrated. Full `cargo xtask ci` passed all
28 test binaries, including 74 I/O tests, 107 document tests, 559 engine tests,
198 kernel tests, 304 UI tests, all 24 native fixtures and ten-crate WASM checking.
The HardEdgeBasin opening regression passed in 0.86 seconds. All twenty local
F3D import groups and the legacy timeline/API comparison passed separately. The
actual imported start-offset cylinder was rendered and visually checked.


### Legacy analytic geometry and split entity records

The geometry reader now accepts verified layout-4/5/8 point, line, circle and arc
schemas. All 775 points and 599 analytic curves from the three larger supplied
files match public API coordinates and fixation; curve comparisons also check
construction, radii and arc endpoints. The acceptance test reads both the original
archives and their modern Fusion-resaved versions. It does not require a running
Fusion installation, and no generated archives enter Git.

Inherited curve metadata (`9FF71F46-205F-484B-A2B5-A245D0D8FE33`) versions 8/9 differ
by a byte before the construction flag. Geometry version 17 adds a counted reference
list before the owner. The former opaque curve tail contains a variable relationship
list; these references are now bounded, validated and retained. Unknown point flags
are exposed separately instead of receiving guessed constraint meanings.

Specialized point `00D157C2-2332-4BD5-BB53-048C4081D1C3` and line
`16DEFC4D-1816-4FB0-8E39-9BDA23954248`, both version 0, split attributes and geometry
between primary and secondary records. Parent classes, framing, optional definition
references and duplicated endpoint references are validated. Sixteen specialized
lines and twenty-two specialized points in the adapter are included in the API
comparison. Their dependent definitions remain explicit; reconstruction currently
rejects them rather than flattening their cached geometry.

A finite line's support axis can oppose its endpoint order. Both parallel
orientations are valid, but a nonparallel or nonunit axis still fails. An original
record regression checks the correct rectangular extrusion and volume after a
width edit with the reversed axis. Additional authored tests cover versioned
construction flags, split-record truncation, conflicting endpoints, retained
relationships and unknown point metadata. Existing supported imports remain
editable. Larger-file constraint and feature reconstruction is still unfinished.

Validation: upstream `af536a7` remains integrated. Full `cargo xtask ci` passed
28 test binaries, including 79 I/O tests, 107 document tests, 559 engine tests,
198 kernel tests, 304 UI tests, all 24 native fixtures and ten-crate WASM checking.
All twenty local F3D import groups and nine local inspection groups passed.
The HardEdgeBasin opening regression passed in 0.88 seconds.

### Legacy constraint containers and editable symmetry

The inspector now exposes `constraint_graphs` separately from supported constraint
semantics. Geometry versions 16, 17 and 18 have distinct framing for counted
relationship/dimension lists, optional support references and external identities.
Every reference is retained and checked. All eighteen containers in the three
larger designs agree before and after a modern Fusion save: 572 geometric
relationships and 195 dimensions in total. Unresolved auxiliary/support links
still prevent reconstruction; decoding a reference list does not imply support
for every constraint in it.

Generic geometric kind `0x400` has three operands: the symmetry axis followed by
the two related entities. Nine independently authored public-API controls add and
remove symmetry between points, finite lines and full circles. Imported circles
retain both center symmetry and equal radii. Imported lines pair endpoints using
complete reflected incidence, allowing either native line direction and rejecting
inconsistent geometry. Arc symmetry remains unsupported. The public
[addSymmetry API](https://help.autodesk.com/cloudhelp/ENU/Fusion-360-API/files/fusion_GeometricConstraints_addSymmetry.htm)
provides the independent behavioral reference, not a binary-format specification.

Three further controls extrude two symmetric circles and independently vary
diameter and height. Native imports match API volumes and retain diameter edits,
serialization/reopening and undo. Original field-authored records test circle
edits, both line directions, invalid axis types, truncated symmetry payloads and
all three constraint-container schemas. No Fusion-generated records enter Git.

The twenty-fifth original fixture, `symmetric-cylinders`, extrudes two 20 mm
diameter circles by 30 mm, for a total volume of 6,000π mm³. Diameter and height
edits check analytic volumes, world bounds, reopening and undo in ordinary CI.
Offset/angular dimensions, other constraint schemas and the larger designs'
remaining feature types still require editable reconstruction.

Validation: upstream `6ce472c` is integrated without conflicts. Full `cargo xtask
ci` passed all 28 test binaries, including 82 I/O tests, 107 document tests,
560 engine tests, 198 kernel tests, 304 UI tests, all 25 original fixtures and
ten-crate WASM checking. All 21 local import groups and ten inspection groups
passed. The HardEdgeBasin opening regression passed in 0.87 seconds. The actual
two-cylinder import was rendered after a diameter edit and visually checked.

### Editable point-to-line and parallel-line offset dimensions

The linear-dimension class also represents perpendicular distances from a line
to a point or another parallel line. These records retain the same validated
owner, binding and complete operand back-references as point-to-point dimensions.
The verified orientation flag is `0x4000`; operand types distinguish the semantics.
Reversed point/line operands are normalized without replacing native parameter
names or expressions. Parallel-line offsets preserve parallelism and distance
as editable constraints. Unknown type/orientation combinations fail explicitly.

Twenty-one local public-API controls independently vary point side, line direction,
line operand order and distance. A control without an explicit parallel constraint
checks that line-offset dimensions preserve parallelism themselves. Imports and
subsequent edits match measured world bounds as well as volumes, then survive
serialization, reopening and undo. Bounds are essential for the point-to-line
cylinder controls, since their volume stays constant when their centers move.

Two original record tests exercise dependent expressions, both operand orders,
invalid flags/types and every truncated dimension payload. The twenty-sixth native
fixture, `offset-dimension-prism`, is an original 30 × 20 × 17 mm prism driven by
the distance between parallel sketch lines. Its 10,200 mm³ analytic volume and
bounds are checked through width, depth and height edits in ordinary CI.
No Fusion-produced file is committed. Angular dimensions, offset-curve generators
and the larger designs' remaining feature histories are still unfinished.

Validation: upstream `8cce89d` is integrated; its generated license-manifest
conflict was resolved by regeneration from the merged lockfile. Full `cargo xtask
ci` passed all 28 test binaries, including 84 I/O tests, 107 document tests,
560 engine tests, 198 kernel tests, 304 UI tests, all 26 original fixtures and
ten-crate WASM checking. All 22 local import groups and ten inspection groups
passed against the integrated build. The HardEdgeBasin opening guard passed in
0.91 seconds. The imported parallel-line-offset prism was rendered after an edit
and visually checked.


### Angular dimensions and independently fixed line supports

Angular-dimension class `6EEAFDE5-59C4-424D-8EBE-77CDBC562017`, versions 0/1,
retains two line operands, owning sketch, native parameter binding, cached radians
and two quadrant fields. Quadrants 0/2 measure the direct angle, while 1/3 measure
the supplement. Opposite annotation quadrants can differ while retaining the same
constraint semantics. Distinct quadrant parity is essential at 90°: identical
cached geometry must move in different directions when its parameter changes.
The importer checks the angle against both the bound parameter and finite native
line directions, then preserves clockwise direction and the supplementary branch.
Unknown flags, inconsistent quadrants, wrong owners and truncated records fail.

Twenty-seven fully constrained triangular-prism exports cover acute, obtuse,
clockwise and reversed-line dimensions and all four 90° quadrants. Eight sketch
exports check edits from 0° and 180°. Their independently measured geometry,
solid volumes and bounds are checked after import, edits, reopening and undo.
Earlier underconstrained controls remain useful for validating imported snapshots,
but their arbitrary free-endpoint positions are not exact edit oracles.
The offset edit controls likewise now fix their reference endpoints explicitly;
the earlier archives and measurements are retained separately. The independent
free-endpoint test prevents this stronger oracle setup from hiding overconstraint.

Those controls revealed an existing fixation mismatch: native line fixation holds
the infinite line support while endpoint fixation remains independent. The
importer previously immobilized both endpoints. It now preserves the support
using a fixed anchor, point-on-line and orientation constraints. Three additional
public-API controls fix one endpoint and drive the other through length edits.
Original record tests also prove that an extruded rectangle's fixed support
allows its width to change. Circle and arc fixation semantics are separate.

The twenty-seventh original native fixture, `angular-prism`, is a 30 × 20 × 17 mm
parallelogram prism. Its analytic volume is width × side × sin(angle) × height.
Edits from 90° to 60° and 120° have equal volumes but different bounds; both are
checked along with height edits, serialization and undo. No Fusion-generated
files are committed. Larger feature histories remain incomplete.

Validation: upstream `4af2d06` is integrated without conflicts. Full `cargo xtask
ci` passed 29 test binaries, including 88 I/O tests, 107 document tests, 560 engine
tests, 198 kernel tests, 304 UI tests, all 27 original fixtures and ten-crate WASM
checking. All 24 local import groups and ten inspection groups passed. The
HardEdgeBasin opening guard passed in 1.41 seconds. An imported triangular prism
was rendered and visually checked after a supplementary-angle edit.

### Fixed arc supports with independently editable endpoints

Native arc fixation now reconstructs a fixed center and a constant radius
constraint, preserving the independent endpoint fixation flags. Setting the
whole imported curve fixed incorrectly held both endpoints and silently left
the extrusion at its previous shape after a driving endpoint-dimension edit.

Three local public-API controls use a radius of 30 mm, center (0, 0), fixed start
(30, 0), and a free endpoint whose horizontal coordinate changes from 10 to 20 mm.
For an extrusion height of 10 mm the independent volume changes from
1296.6766909 to 430.7070513 mm³. Native acceptance checks compare center, start,
end, volume and bounds after editing, reopening and undo. Original field-authored
records exercise the same reconstruction in ordinary CI, including a third
endpoint position. The twenty-eighth original `.solvecraft` fixture,
`fixed-arc-support`, checks the analytic circular-segment volume and bounds for
two endpoint edits and a height edit. No Fusion-generated data is committed.

Legacy support links, offset-curve generators and other feature types remain
unfinished; the larger supplied archives still cannot be fully imported as
editable history.

Validation for the arc correction: full `cargo xtask ci` passed all 29 test
binaries, including 89 I/O tests, 560 engine tests, 198 kernel tests, all 28
original history fixtures, the HardEdgeBasin opening guard and ten-crate WASM
checking. All 25 local F3D import groups passed. The edited imported segment was
rendered and visually checked. Upstream `4af2d06` remains integrated.

### Symmetry roles independent of graph registration order

Symmetry records have two lists with different purposes: graph operands carry
connection slots, while the ordered references identify entity one, entity two
and the mirror axis. Reconstruction now uses the ordered references after
checking that both lists contain the same unique objects. Previously an axis
created after a mirrored entity was mistaken for an entity itself; circle/point
imports failed, and line imports could use the wrong mirror line.

Thirty local controls cover points, lines and circles, axes created first,
between the entities or last, and both API entity argument orders. Circle controls
also change and restore a driving diameter; tests compare volume, bounds,
mirrored positions and the actual axis after edits, reopening and undo. Original
record tests permute all six graph operand orders independently of the two entity
orders, and reject malformed ordered references. The existing original symmetric
cylinder fixture continues to cover editable geometry in ordinary CI.

The same ordered references match all 113 symmetry constraints reported by the
public API for the supplied files: nine in the adapter and 104 in Gridfinity.
Nine adapter and 88 Gridfinity constraints put the axis outside the first graph
operand position. Gridfinity's constraint decoding now advances to the retained
auxiliary relationship blocker. Adapter also contains line-to-line midpoint
relationships that still need reconstruction; R36S retains body support links.
Those and the remaining feature schemas still prevent full editable import of
the larger archives. Fusion controls and API snapshots remain local evidence.

Validation for symmetry ordering: full `cargo xtask ci` passed 29 test binaries,
including 90 I/O tests, all 28 original history fixtures, the HardEdgeBasin
opening guard and ten-crate WASM checking. All 26 local import groups passed.
The edited imported cylinder pair was rendered and visually checked; upstream
`4af2d06` remains integrated without additional commits to merge.

### Common line midpoints and converted dimension parameters

Generic kind `0x1000` with two line operands equates their centers while leaving
their lengths and directions independent. Reconstruction uses an unfixed shared
point with a midpoint constraint on each line. Native centers must agree before
import; missing, repeated, wrong-type and truncated references fail explicitly.
Two original record tests check operand order, rotation, moving reference centers,
dimension edits, volume and reopening.

Five local controls isolate an existing legacy midpoint relationship. Fusion
blocks translation of one center away from the other, allows independent rotation,
and permits translation after removing the constraint. Six triangular-prism
controls check parameter edits, expression dependencies, world bounds, analytic
volume, reopening and undo. Those controls also expose dimensions converted to
user parameters after deletion of their original features. Modern parameter
metadata preserves that role separately from explicit user parameters. An export
can retain a reference to a deleted binding until resaved; the importer accepts
that reference only when the conversion flag is present and the binding object
is absent. Live conflicting bindings and inconsistent role flags fail.

The twenty-ninth original fixture, `line-midpoint-prism`, is a 30 × 20 × 17 mm
prism whose bottom-edge midpoint follows a construction line. Its 10,200 mm³
analytic volume and world bounds are checked after width, depth, height and
reference-elevation edits, serialization, reopening and undo. Elevation changes
preserve volume while moving the body. Fusion exports remain local evidence.

Thirteen further archives from the supplied knob/clamp ZIPs have been inspected.
All use layout 5; their indexes expose 156 timeline entries in total. Unsupported
legacy entity framing, auxiliary relationships, extent schemas and additional
feature types still prevent full editable imports. The smaller controls establish
specific supported behavior; they do not imply general F3D compatibility.

Validation: upstream `d8beaed` is integrated without conflicts. Full `cargo xtask
ci` passed all 29 test binaries, including 94 I/O tests, 560 engine tests,
198 kernel tests, 305 UI tests, all 29 original history fixtures and ten-crate
WASM checking. The HardEdgeBasin opening guard passed in 0.87 seconds. All 28
local import groups and ten inspection groups passed. The edited imported
triangular prism was rendered and visually checked.


### Associative circle and closed-line offsets

Offset generators now retain their signed distance parameter, source curves and
output identities for circles and connected closed line loops. Reconstruction
checks both group ownership and every regenerated output point/radius against
the stored geometry. Generator dependencies are ordered before reconstruction;
cycles, duplicate producers, missing references and truncated records fail.
Relationship reference order can change from interleaved to grouped after an
edit; correspondence comes from the separately validated source/output groups.
Connection slots are graph registrations, not curve directions.

Fifty-six local public-API controls cover current and legacy offset creation,
inward/outward circles and rectangles, shifted circle centers, distance edits,
source edits and restored values. Forty closed-profile controls exercise editable
imports, volume, bounds, reopening and undo. Sixteen open-line controls establish
an important remaining limitation: native open offsets retain independent endpoint
extents when their source grows. SolidCraft's current analytic chains extend with
the source, so native open offsets are explicitly refused pending reconstruction
of those endpoint freedoms. Mixed arc chains and other unverified generator
schemas also remain unsupported.

Design format 15 retains optional analytic offset distance expressions; older
numeric offsets keep their values. Expressions participate in dependency tracking,
parameter usage/deletion checks, renaming, recomputation and serialization. The
`sketch.offset` command accepts length expressions for supported analytic offsets.
Circle outputs with distinct center points follow source-center edits.

The thirtieth original fixture, `offset-circle-ring`, has radius 30 mm, inward
gap 5 mm and height 12 mm: volume π(30² − 25²)12 = 3300π mm³. Independent gap,
source-diameter and height edits check analytic volume and world bounds through
reopening and undo. The native importer also has original field-authored record
tests, including malformed inputs. Fusion exports and screenshots stay local.

Validation: upstream `e171ff0` is integrated without conflicts. Full `cargo xtask
ci` passed 29 test binaries, including 97 I/O tests, 107 document tests,
561 engine tests, 198 kernel tests, 305 UI tests, all 30 original history fixtures
and ten-crate WASM checking. The HardEdgeBasin opening guard passed in 0.86
seconds. All 29 local native-import groups and ten inspection groups passed.
An imported offset ring was rendered and visually checked after a gap edit.


### Legacy offset groups and multiple selected regions

The layout-4/5 offset group secondary stream retains an additional zero field
before its generator owner; current Fusion resaves remove it. Offset dimensions
may select any curve in the source group, independently of the generator's
selected curve. Both references are validated against the complete source group.
All six generators and the complete Gridfinity sketch-constraint list now decode.
An independent public-API comparison verifies every parent/output curve and its
signed parameter in both the original and resaved archives. Original record tests
cover the legacy field and refuse nonzero or malformed values.

Profile tracking now retains a bounded list of selected regions, each with its
own outer loop and holes. Disconnected regions from one sketch produce separate
bodies in one editable extrusion. References and holes are preserved across
parameter edits rather than selecting profiles by their transient indexes.
Duplicate regions, malformed flags and truncated tracking records are rejected.
Six local circle/ring controls check individual body bounds, summed volume,
diameter/height edits, reopening and undo. Original field-authored tests provide
normal-suite decoding and malformed-input coverage.

The same complete-boundary framing is verified for nine legacy records: six in
the adapter and three in Gridfinity, equal to their current Fusion resaves.
Other records contain split/intersecting curves or unresolved tracking identities;
those still require reconstruction. This does not establish full import of the
larger archives. Body-support links, other feature types, open offset endpoints
and remaining legacy geometry schemas are also unfinished.

Two new original fixtures, `multiple-circles` and `multiple-rings`, bring the
editable-history suite to 32 projects. Radii 10/20 mm and height 12 mm give
6000π mm³ for the cylinders; holes of radii 5/10 mm reduce the rings to 4500π.
Both fixtures check diameter and height edits, body count, bounds, reopening and
undo. Fusion exports remain local evidence.

Validation: full `cargo xtask ci` passed 29 test binaries, including 100 I/O
tests, 107 document tests, 561 engine tests, 198 kernel tests, 305 UI tests,
all 32 original fixtures and ten-crate WASM checking. The HardEdgeBasin opening
guard passed in 0.87 seconds. All 30 local import groups and 12 inspection groups
passed. The imported two-ring model was rendered and visually checked after
diameter and height edits. Upstream `e171ff0` remains current and integrated.


## Split analytic profile boundaries

Fifteen local public-API controls isolate a lens and both crescents from overlapping
circles, and both halves of a rectangle crossed by a line with dangling ends. Each
control has a source-dimension edit and a subsequent extrusion-height edit. The two
tracking integers previously required to be `1, 1` are a one-based source piece
number and the total source piece count. The independent reversal flag controls
traversal. The divider's middle segment is piece 2 of 3, even though its two outer
pieces cannot bound a closed profile. A circle's first piece wraps across its
positive-X parameter seam; the other pieces follow positive angular order.

The reader retains these fields, validates their bounds, and distinguishes pieces
of the same source curve. Split boundaries import as format-16 `PieceBoundaries`
references, with exact outer and hole identities. The sketch graph certifies those
identities before pruning dangling edges; certificates are recomputed rather than
loaded from serialized profile caches. Recompute requires the same source IDs,
piece numbers, counts and traversal directions. Missing or ambiguous boundaries
produce a model error rather than choosing a nearby region by list index.

This initial split-boundary step certified native lines and positive-Z circles
with the observed positive-X seam. The later controls below extend it to partial
arcs, rotated native circle seams and cuts on the seam. Free-form tessellation
still cannot acquire an analytic-piece certificate.

Five original command recipes and `.solvecraft` fixtures cover the lens, both
crescents and both divided-rectangle regions. Expected circle areas use the
analytic two-circle intersection formula; rectangle volumes use width × depth ×
height. They exercise source edits, height edits, save/reopen and undo. Original
field-authored native records additionally check all three overlapping-circle
regions, malformed counts, every truncated tracking prefix, and topology changes
that remove the selected intersection. No exported Fusion file is committed.

The same bounded metadata decoder now matches sixteen profile records between the
original legacy archives and their current Fusion resaves: ten in the adapter and
six in Gridfinity. This is a metadata correspondence check, not a claim that those
entire documents import successfully. Larger feature histories remain incomplete.

Validation for this step: full `cargo xtask ci` passed all 29 test binaries,
102 IO tests, 109 document tests, 561 engine tests, 198 kernel tests, the
37-fixture history suite, and the ten-crate WebAssembly check. The HardEdgeBasin
opening guard completed in 0.86 seconds. All 31 local native import groups and
12 native inspection groups passed separately. Upstream `8fa52c8` is integrated;
the edited crescent was rendered and visually checked using original fixture data.


## Rotated seams and partial arcs

Sixty new local public-API exports isolate partial semicircles at 0°, 60° and
180°, circles cut into four regions at centered and offset crossings, and
intersecting circles with source axes rotated by 60° and 180°. Each baseline
has a source-dimension edit and a subsequent height edit. All imports are compared
with independent Fusion volume, body-count and bounding-box measurements; edits,
reopening and undo are also checked. No Fusion exports are committed.

Circle piece numbering follows the saved local X axis. The first interval contains
that seam; when a cut lies exactly at the seam, the first interval starts there.
Arc pieces number forward from the source start point regardless of its rotation.
Dangling line portions still contribute to the source count. These observations
replace the previous restrictions on rotated circle seams and partial arcs.

Format 17 stores optional `circle_seams` in `PieceBoundaries` references, measured
in sketch-local radians. Missing seams preserve format-16 positive-X numbering.
Seam values must be finite, in [0, 2π), and refer to selected circle sources.
Runtime graph partitions remain private, shared and unserialized; recomputation
resolves the saved parameter frame against all current cuts. Exact seam lookup
checks only adjacent cuts using binary search, avoiding a full cut scan for each
referenced piece. Invalid, missing and ambiguous boundaries still fail explicitly.

Rotated native semicircles may save an end angle about 2 × 10⁻⁸ radians from the
independent endpoint. The importer permits radius-scaled numeric roundoff while
retaining the referenced endpoint, and rejects larger geometric disagreement.
Normal vectors within 10⁻¹⁰ of positive Z are accepted. These tolerances do not
permit arbitrary off-plane sketch geometry.

Five original `pieces-*` recipes and saved `.solvecraft` files bring the suite to
42 projects. They cover a rotated-seam lens and crescent, an exact-seam quarter,
and both portions of a split semicircle. Volumes use analytic sector and circle
intersection formulas; source and height edits, reopening and undo are exercised.
Free-form profile certificates, other feature types, body-support dependencies,
assemblies and remaining legacy schemas are still unfinished.


Validation: full `cargo xtask ci` passed all 30 test binaries, including 104 I/O,
113 document, 565 engine, 204 kernel and 319 UI tests, all 42 original history
fixtures, and ten-crate WebAssembly checking. All 32 local native import groups
and 12 inspection groups passed. The HardEdgeBasin guard passed; an isolated
`bench-open` measured 13.0 ms parsing, 863.6 ms evaluation and 14 displayed bodies
with the existing eight expected unsupported endpoint fillets. Upstream `07e263d`
is integrated, with both branches' performance documentation retained. The edited
rotated-arc fixture was rendered and visually checked.


## Legacy offset planes

Gridfinity's original version-2 construction-plane features and version-8
plane definitions now decode alongside current version-4/15 records. Its public
API identifies two ordinary offsets from XZ and YZ, driven by `d188` and `d94`.
Original and resaved records retain identical feature/geometry/base identities,
distance bindings and cached placements. The extra-offset slot is optional;
absence remains absence rather than introducing an artificial driving parameter.

Plane feature headers use the shared optional-attribute block. The observed
version-8 definition has a shorter zero header and lacks the later method flag;
all other reference, method, ownership and finite-placement checks remain active.
`file.inspect_f3d` reports these offset dependencies even when another timeline
feature prevents importing the whole design. This raises the decoded Gridfinity
timeline from 15 to 17 of 35 entries; full Gridfinity import remains unfinished.

Original field-authored old/current records check chained parent edits, reopening
and every truncated feature/definition prefix. A new original YZ fixture checks
positive-X extrusion bounds, offset edits and undo, bringing the editable-history
suite to 43 files. No exported Fusion data is committed.

The next loft investigation has six measured two/three-section controls. The
adapter's four public-API lofts combine an existing body face with a sketch profile
and use join operations. Three-section controls show Fusion's smooth interpolation;
matching their history requires both source-face references and verified loft
geometry, not simply a series of frustums. Loft import is not implemented yet.


Validation: full `cargo xtask ci` passed 30 binaries, including 105 I/O,
113 document, 565 engine, 204 kernel and 319 UI tests, all 43 history fixtures,
and ten-crate WebAssembly checking. The HardEdgeBasin guard passed in 0.86 s.
All 32 local native import groups and 13 inspection groups passed. Upstream
`07e263d` remains integrated and current. Opening Gridfinity now gets past both
plane entries and reports its next unsupported timeline class, FilletFeature
at object 46870; this is not yet a full-document import.


## Ordered two-section lofts

Version-9 loft features now reconstruct two free sketch-profile sections as an
editable NewBody loft. The ordered section wrappers are version 1 of class
`2F3200BA-FDFB-48C6-97D4-71A4A0D86190`; each explicitly names its owning feature
and profile container. Sketch-profile ownership accepts either an extrusion or
loft. Section IDs are never sorted to infer order. Suppression and section
sketch/plane dependencies survive saving and reopening.

Independent local controls distinguish solid/surface and merged/unmerged flags,
reverse the section order, suppress the feature, and change source diameter and
section-plane height. Reconstruction requires the observed solid, merged,
free-condition schema and a single region without holes in each section. Surface,
face/point, boolean, guided and other unverified modes fail explicitly. Compact
identity plane transforms now decode alongside explicit 4×4 matrices; mixing an
origin sketch with an offset-plane sketch exercises this representation.

Three-section lofts remain rejected during reconstruction. The existing kernel's
uniform interpolation gives approximately 59,057.5 mm³ for the baseline control,
close to Fusion's measured 59,038.4 mm³, but its maximum radius differs by about
0.044 mm. The volume tolerance alone would conceal that discrepancy. Body-face
sections and their persistent parent dependencies are also still required by the
supplied adapter, so this does not yet enable importing that complete file.

Three original circular/reversed/rectangular loft recipes and `.solvecraft`
fixtures bring the ordinary editable-history suite to 46 designs. Tests use
analytic frustum volumes and check dimension/plane edits, reopening and undo.
The native decoder tests use original field-authored records, owner/section
corruption and truncated prefixes; no Fusion-produced archive is committed.

Validation: full `cargo xtask ci` passed, including all 46 original fixtures,
108 I/O tests and ten-crate WebAssembly checking. All 33 local native import
regression groups passed separately. The rectangular loft fixture was rendered
and visually checked. Upstream `07e263d` remains integrated and unchanged.
