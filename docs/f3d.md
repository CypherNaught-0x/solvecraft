# Native F3D investigation

Status: bounded archive inspection and **editable import of verified layout-8
circle/line/arc sketches and one-sided-extrusion history**. General F3D history support remains
incomplete; the three larger supplied designs do not import yet.
The project owner authorized clean-room investigation on 2026-10-11 as an
exception to AGENTS.md's public-specification requirement. Only the supplied
files and locally generated controlled variants were observed through Fusion's
public API; no proprietary or GPL/LGPL/AGPL implementation was used.

`file.inspect_f3d {"path":"...f3d"}` validates the archive and checks the native
object index against its bulk records, leaving the current design untouched:

```sh
cargo run --profile ci -p solvecraft-cli -- exec file.inspect_f3d '{"path":"/path/to/part.f3d"}'
```

`doc.open` reconstructs supported sketches, constraints and extrusions and
regenerates the model before replacing the current document. Unsupported schemas
or regeneration failures leave the current document/model/undo state intact.
An imported document is unsaved with no source path: saving it uses a new
`.solvecraft` file, never JSON written over the original F3D. The desktop open
dialog includes F3D, and the CLI supports opening, measuring and benchmarking it.

Supported reconstruction includes native parameter names and expressions,
driving diameter and horizontal/vertical distance dimensions, coincidence and
horizontal/vertical constraints, independent point/curve fixation, construction curves, rigid root sketch placement, feature names/order/suppression,
complete outer and inner boundaries for circle, polygon and arc profiles, signed extrusion distances and taper. New-body
operations and booleans against a sole existing body are supported. Multiple-body
boolean targeting, two-sided/start/to-object extents, other sketch geometry and
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
Bindings (`D91D429C-1BB5-4640-B49F-69AF14F463E9`, version 0) connect parameters to
their owning objects. Both binding back-references and duplicate numeric values
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
  object ID is not its profile tag. The verified schema is a single loop containing
  full circles or chains of finite lines and arcs, including inner loops. Intersected partial-curve boundaries still fail explicitly. Ownership and back-references
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
intersected partial-curve profiles, assemblies and native B-reps remain unresolved. Parameter
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

Thirteen [small editable SolveCraft fixtures](../apps/solvecraft-cli/tests/history/README.md)
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
