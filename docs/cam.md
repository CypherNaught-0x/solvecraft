# Three-axis milling and independent validation

M15 is in progress. The implementation is an experimental first milling workflow; **Fusion
parity has not been established**. No output is qualified for a physical machine controller.

## Architecture and commands

`solvecraft-cam` is pure Rust at L1 and depends on geometry. The document persists manufacturing
in format 12; earlier designs migrate with an empty manufacturing section. Generated paths and
verification results are session caches and are deliberately absent from saved designs.
The engine resolves bodies, explicit occurrence paths and durable sketch-profile curve identities
into immutable snapshots in setup coordinates. Missing or ambiguous references require reselection.
Any document or pending occurrence-placement edit makes the cached job stale; undo can restore it.

The Manufacture workspace, CLI and MCP share these registered commands:

| Command | Input / behavior |
|---|---|
| `cam.setup.put` | `{setup: Setup}`: rectangular stock, right-handed rigid frame, model/fixture references, clearance, retract, G54–G59 |
| `cam.tool.put` | `{tool: Tool}`: document-local center-cutting flat mill or twist drill, cutting/shank/holder dimensions |
| `cam.boundary.from_profile` | `{setup, sketch, profile, occurrence?}`: returns sampled boundary and durable source reference |
| `cam.operation.put` | `{operation: Operation}`: face, contour, pocket or drill; explicit feed, plunge, spindle and depth settings |
| `cam.program.put` | `{program: {id,name,setup,operations}}`: ordered operations, one setup/work offset |
| `cam.operation.order` | `{program, operations}`: permutation of existing operations |
| `cam.delete` | `{kind,id}`: refuses removal while referenced |
| `cam.generate` | `{program, background?}`: immutable job; returns job ID/status |
| `cam.job.status`, `cam.job.cancel` | `{job}` |
| `cam.inspect` | `{job?, include_path?, include_stock?, motion?, finding?}`: world-space stock display mesh and machine state after the requested number of motion blocks |
| `cam.simulate` | `{job, background?}`: independent post round trip and stock/clearance checks |
| `cam.post` | `{job,path?}`: requires fresh generated paths and passing verification |
| `cam.export` | `{job,format,path?}`: `toolpath`, `setup_sheet`, `stock`, `validation` |

All dimensions are mm; speeds are mm/min and rpm; angles in the command model are radians.
The UI displays tool and ramp angles in degrees. IDs must be nonzero and unique per item kind.
See the serializable types in `crates/cam/src/model.rs` and original fixtures in
`crates/cam/src/corpus.rs` for complete payloads. Without a path, exports return `content`.

Facing cuts stock with raster passes. Contouring uses geometric inside/outside cutter-radius
offsets. Pocketing uses successive offsets, handles islands, and retracts between loops.
Drilling expands every peck into ordinary motions and distinguishes tip depth from full-diameter
depth. Contours/pockets can use tangential leads and feed-controlled ramps. Allowance means stock
to leave; use a separate zero-allowance operation for finishing. Flat mills are explicitly assumed
to be center cutting. Entry/lead collisions are checked during simulation, before posting.

Offsets use exactly `cavalier_contours 0.9.0` (MIT OR Apache-2.0) behind bounded input checks.
Crossing/touching loops, nested islands, oversized tools, non-finite values and exceeded budgets
are errors. The offset adapter uses 1e-8 mm position/join and 1e-7 mm distance tolerances;
these avoid merging small distinct slices in round-island pockets with upstream defaults. Output retains operation IDs, machine events and linear/planar-arc motions.

## Reference NC dialect

The post emits tool-tip coordinates, three linear axes, absolute mm positioning, XY arcs with
incremental I/J centers, units-per-minute feed and geometrically applied cutter compensation.
It explicitly initializes G21/G90/G17/G94/G54–59/G40/G49/G80, emits tool/spindle/coolant events,
and expands drilling. Startup is a declared position at clearance; physical travel to that
initial pose is outside this reference dialect's contract. There is no machine kinematics,
G28/G53 homing, length offset activation, canned cycle or controller-specific behavior.

The separate NC reader consumes every word and rejects unsupported commands. It supports G20
and G91 for comparison, but requires complete absolute XYZ for the initial pose. Modal groups,
units, feed, tool changes, spindle state, planar arc radii and program termination are checked.
Program end includes spindle/coolant shutdown in the interpreted event stream. Posted motions
are compared against the pre-post typed IR with a 0.001 mm error budget.

Semantics reference: [NIST RS274/NGC](https://tsapps.nist.gov/publication/get_pdf.cfm?pub_id=823374).
The reader is a deliberately strict subset, not a general RS274 interpreter.

## Verification and present limits

The independent verifier uses adaptive XY cells with conservative lower/upper stock heights,
flat-mill or drill-cone removal, local target-surface checks, and swept rapid/shank/holder bounds.
Fixture checks use pinned `parry3d-f64 0.31.1` (Apache-2.0): separate cutter, drill cone, shank and
holder shapes are swept against fixture triangles. Initial-distance and closed-volume winding
checks catch stationary contact and tools already inside fixtures. Body groups preserve enclosed
cavities and prevent opposite winding in separate fixtures from cancelling. Open/nonmanifold
meshes, clearance within the interpolation/refinement margin, and failed numerical queries remain
**inconclusive**. It does not call offset or toolpath-generation algorithms and observes cancellation
during topology preparation and collision queries. The current algorithm fingerprint is `cam/9`.
No target fitting, translation, unit guessing or discarded unknown NC commands is allowed.

Default dimensional tolerance is 0.01 mm; interpolation/post error is 0.001 mm; requested spatial
refinement is 0.0025 mm. Reports distinguish requested refinement from the actual maximum stock
height interval. Limits are 250,000 cells and 30 million work units by default. Exhaustion or an
unresolved collision blocks posting. Remaining-stock volume is an interval, not an exact number.

Stock STL now contains a conforming exterior of the upper height envelope. Shared edges and
vertical steps are split, internal walls are removed, and every edge and vertex link is checked
for a closed, consistently oriented manifold. Coplanar cells merge exactly without resampling.
Exports are bounded to 40,000 merged cells, 500,000 triangles and 128 MiB. Non-manifold touching
stock is rejected explicitly; validation JSON retains the cell intervals. `stock_complete` is
false after verification budget exhaustion, which blocks export of a misleading partial stock.
A watertight upper-bound mesh does not establish finished-part dimensional accuracy.

Stock comparisons require declared stock bounds and exact, non-overlapping cell coverage. Older
reports without bounds remain inconclusive. Imported reference meshes must be closed, consistently
oriented manifolds within those bounds, with downward faces only on the stock bottom. Unsupported
undercuts, reversed faces, degenerate triangles and out-of-frame vertices are rejected. Projection
coverage uses boundary incidence and pinned `robust 1.2.0` orientation predicates, not sums of
triangle areas. Duplicate or overlapping faces cannot counterfeit coverage. Extremely small nonzero
XY coordinates below 1e-100 are rejected to avoid predicate underflow; coordinates are never snapped.

The comparator independently bounds reference heights from triangles, reusing only the validated
spatial partition before further adaptive subdivision. It certifies whole-column distances for
both remaining material and removed space, in both directions, including the stock exterior.
This resolves narrow uncertainty bands at vertical walls without concealing an omitted deep hole.
Failures require a definitely present material/void point farther than tolerance from all possible
corresponding columns; inconclusive intervals are not failures. Distance maxima describe examined
regions; a certified failure may stop at its first witness. Cell/work exhaustion remains inconclusive.
Tests include independent analytic round holes, alternate facet layouts, generated drilling-stock
round trips, shifted walls, missing holes, incomplete coverage and malformed reference meshes.

The current passing verification status certifies the implemented no-gouge/clearance checks.
The separate `finished_dimensions` status certifies facing planes, analytic drill cylinders/cones,
pocket floors and islands, and the selected side of contour walls. The milling verifier uses
independent robust winding and point-to-segment distances, with whole-cell signed-distance bounds;
it never calls the generator's offset functions. A contour must finish its selected wall and floor
within one tool radius of the allowance boundary. Farther stock on the cutting side may remain as
roughing stock; it cannot be cut below the specified floor. A pocket must clear its full selected
interior to the allowance floor while preserving its islands and outer wall.

The radial tolerance band reserves half the dimensional tolerance for paired-wall/diameter error,
then subtracts the post/interpolation budget. Associated CAD profiles also reserve their chord-error
budget, at most 0.0005 mm (and at most 1/16 of operation tolerance), during engine sampling and
independent certification. Requested features drive adaptive refinement even if the NC omits them.
Multiple operations compose by taking the minimum permitted remaining-height envelopes.
Drilling requires zero stock allowance: use explicit depth, since drill paths do not apply milling
allowance. Sharp inaccessible internal corners, omitted islands, wrong tools/depths and shifted
paths fail dimensional validation and block posting. Exhaustion and unresolved bounds stay
inconclusive. Posting requires both clearance and finished dimensions to pass and complete stock.

Motion queries use an independent XY index and retain original sequence IDs for withdrawals and
machine-state checks. Exact item bounds exclude irrelevant candidates; shank/holder radii remain
included whenever those components can reach stock. Region predicates have bounded work and
cooperative cancellation. The analytic contour/pocket recipes now use an original 128-sided round
boundary and matching protected walls/floors; they no longer rely on a plane-only square smoke test.

Protected height-field boundaries use local spatial certificates. A conservative upper target
height at a neighboring XY point bounds distance to the outside of the protected solid for an
entire potentially removed cell column. A gouge is reported only with a definitely removed point
inside a protected volume by more than tolerance. Clearance reserves 0.001 mm post/interpolation
error plus the 0.0025 mm verifier budget; CAD tessellation requests half that verifier budget.
Queries use an independent XY index, observe work limits and cancellation, and leave unsuccessful
certificates explicitly inconclusive with a `protected_surface_unresolved` finding.

The original drilling fixture now contains the plate top, hole wall and cone, replacing its former
plane-only protected target. Tests cover boundary clearance, shifted-hole and through gouges,
sloping surfaces, overlapping projected targets and large-coordinate precision. A native CAD
drilled body also passes create/generate/simulate/post/save/reopen through registered commands.
This does not establish qualification of arbitrary non-height-field solids or Fusion parity.
Native CAD pocket and contour workflows also pass through durable sketch-profile references,
ramp entry, generation, simulation, posting and save/reopen. All four analytic strategies now
require passing finished-dimension checks; independent point probes supplement those certificates.
The Fusion oracle requires the same dimensional checks for both programs. Remaining M15 gates:

- Broader protected-surface, concave-region and multi-operation dimensional regression coverage.
- Broader fixture regression coverage and performance acceptance; supported stock/model geometry
  remains vertical height-field scope. Fixture triangles must form closed consistently wound bodies
  for volume-containment certification; surface contact can still be detected on an open mesh.
- Browser timing acceptance for worst-case single-operation offset jobs. Background jobs yield
  during preparation, boundary validation, between offset queries and every 256 emitted motions,
  as well as during verification. UI frames or status polls advance browser jobs. One library offset
  query, one body tessellation and input hashing remain atomic; their worst-case browser latency
  is still an acceptance gate. Native background jobs perform profile sampling, tessellation,
  transforms and hashing on the worker from an immutable captured snapshot. Synchronous command
  calls remain available. Status distinguishes `preparing` from `running`; the fingerprint is null
  until preparation completes. Cancellation cannot publish a partial path or revive a cancelled job.
- Associated CAD curves use second-derivative chord-error bounds after placement, including a
  numerical reserve and a shared 10,000-vertex boundary limit. Requests exceeding precision or
  vertex budgets fail explicitly rather than silently loosening the operation tolerance.
- Visible-UI regression coverage for all strategies and all local Fusion reference cases. Native
  facing generate/simulate/post/save/reopen/edit/regenerate has been exercised, including stale-result
  feedback after changing coolant.

These gates must be closed before calling M15 complete or claiming Fusion machining parity.

## Reproducible comparisons

Run `cargo xtask cam-validate`. Original analytic recipes generate input snapshots, canonical paths,
NC, stock, validation JSON and an HTML report under ignored `target/cam-validation/`. Analytic checks
run in ordinary CI and `cargo xtask ci`. Core tests include modal state, inch/mm equivalence, wrong
arc direction, deliberate gouges/unsafe rapids, shank interference, fixture overlap, geometry errors,
islands, ramps and cancellation. Engine tests cover save/reopen, undo and stale-result rejection.

Run `cargo xtask cam-oracle --dir plan/fusion/cam/oracle` for local Fusion packages. The harness
requires separate `face`, `contour`, `pocket`, `drill` directories. Missing evidence is a failure
to establish parity, never a skipped pass. Each directory requires:

- `manifest.json`: nonempty `fusion_version`, `captured_at`, `input_fingerprint`, SHA-256
  `post_sha256`, and `capture_complete: true` only after actual capture.
- `post.cps`, `post-properties.json`, `input.json`, independently captured `reference-input.json`.
  Input snapshots must match exactly, including frames, geometry, tools, allowance and settings.
- `reference.nc`, Fusion `setup-sheet.html`, `setup-sheet-values.json` with `setup`, `tools`,
  `operations` values transcribed from the captured sheet/API, and `simulation.json` with status.
- `stock.stl` in setup millimetres, captured after the operation. Each first fixture has one
  operation; multi-operation coverage must capture and validate every intermediate state.

The harness verifies provenance/input fingerprints and the actual post hash, interprets both
programs, simulates them through the same independent verifier, compares local spatial stock
bounds and the exported reference STL, checks setup values and reports distance/time/volume
deltas. Spatial indexing bounds local comparisons. HTML path overlays link to numbered NC listings and diagnostics. Stock heatmaps show sampled
remaining material relative to protected geometry; they are diagnostic views, not acceptance tests. Intermediate roughing must be judged against protected geometry and allowed stock;
identical roughing paths, retract counts or execution times are not acceptance criteria.

Create geometry from the original analytic recipe in each application. Record Fusion version,
license/capabilities, all resolved settings and capture timestamps. Use its current
[NC Program posting API](https://help.autodesk.com/cloudhelp/ENU/Fusion-360-API/files/PostToolpaths_Sample_Sample.htm)
and [Save Stock](https://www.autodesk.com/support/technical/article/caas/sfdcarticles/sfdcarticles/How-to-save-the-stock-as-an-STL-from-simulation-in-Autodesk-HSM-and-Fusion-360.html)
where no supported automated stock export exists. Do not fabricate missing artifacts or mark
incomplete captures complete. Additional fixtures must cover concavity, multiple islands,
reversed winding, narrow channels, round profiles, offsets, through holes and multiple tools.

All Fusion-produced files and derived reports belong in ignored `plan/` or outside the repository.
The oracle refuses an unignored in-repository destination. Commit only original recipes, harness
code and independently authored expectations. No vendor post source, templates or assets are
part of SolveCraft. The current local capture is incomplete and contributes no parity passes.

## Manufacture display and playback

The workspace shows the selected program's model, initial stock or simulated final stock, with
separate fixture coloring and visibility controls. Stock uses the setup coordinate frame; hidden
model and fixture bodies cannot be selected in the viewport. Editing a setup reveals the bodies
for reference selection. Missing or stale stock falls back to the model with an explicit message.
The remaining-stock display is the final conservative upper surface, not stock removal animated
at each playback step. Its mesh is cached by the simulation job and shares the bounded, conforming
boundary extraction used by STL export. Display extraction errors do not replace simulation status.

Playback advances typed motion/event blocks at an adjustable rate, independent of frame rate.
Zero means the initial clearance pose. The panel shows operation, tool, spindle and tool-tip
coordinates; paths distinguish rapid and cutting motion, including full-circle arcs. `cam.inspect`
exposes the same machine state, and `ui.inspect.manufacturing` exposes the current display choices.
Stock extraction runs on the native worker; its topology phase remains atomic on WebAssembly and
is included in the outstanding browser latency acceptance gate. Native UI tests cover stock-frame
placement, visibility, stale results and playback boundaries. The four-strategy headless UI
walkthrough exercises background generation/simulation, posting, stock selection and playback zero,
with CPU-rendered stock views. Interactive desktop-window coverage remains an acceptance task.


Verification findings include a one-based interpreted NC `record` when a physical line is
known. SolveCraft's independent post round trip also attaches `source` with physical line,
operation ID and one-based IR block (the completed-block count used by playback). Attribution
is added after verification and never influences clearance or dimensional decisions. Raw NC
comments and deserialized data cannot supply this trusted mapping; imported programs retain
line/record attribution without a guessed SolveCraft operation. Arc subdivisions share their
original source block. Program-wide and missing-cut findings may have no responsible line.

`cam.inspect {job, finding: index}` uses a zero-based finding index and returns the reported
world-space point, operation definition, associated CAD source, exact NC line and playback state
where mapped. It rejects stale jobs and invalid indices; failed verification still blocks posting.
The Manufacture panel's **Locate issue** action uses that command to center the reported witness
and seek the completed source block. The marker remains visible when paths are hidden, can be
cleared, and disappears when inputs change. The marked witness is not a claim of the exact first
collision contact, and the stock view remains the final conservative stock surface. Validation
JSON and HTML NC diagnostics retain the operation/block/line attribution.

Findings have explicit information, unresolved or error severity. The bounded list keeps one
representative witness per kind/NC line/severity, prioritizes definite errors when full, and
shows errors first. This list is diagnostic sampling, not an exhaustive inventory of affected
points; verification status and stock intervals are computed independently of finding retention.
