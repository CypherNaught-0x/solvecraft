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
| `cam.inspect` | `{job?, include_path?}` |
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
are errors. Output retains operation IDs, machine events and linear/planar-arc motions.

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
flat-mill or drill-cone removal, local target-surface checks, swept rapid/shank/holder bounds and
fixture bounds. It does not call the offset or toolpath-generation algorithms. It checks
cancellation during refinement. Fixture-bound overlap is **inconclusive**, never clear.
No target fitting, translation, unit guessing or discarded unknown NC commands is allowed.

Default dimensional tolerance is 0.01 mm; interpolation/post error is 0.001 mm; requested spatial
refinement is 0.0025 mm. Reports distinguish requested refinement from the actual maximum stock
height interval. Limits are 250,000 cells and 30 million work units by default. Exhaustion or an
unresolved collision blocks posting. Remaining-stock volume is an interval, not an exact number.

The current passing verification status certifies the implemented no-gouge/clearance checks.
The separate `finished_dimensions` status certifies the entire finished plane for facing-only
programs. It remains **inconclusive for walls and holes**: excess material is reported separately
because intermediate roughing may intentionally leave it. Definite finished-plane errors block posting.
Experimental reference posting for other strategies checks paths; it does not certify dimensions.
The Fusion oracle additionally requires passing finished-dimension checks for both programs. Analytic acceptance currently adds
independent point-height expectations; those probes do not replace whole-region dimensional
certification. This is an open M15 release gate, along with:

- Certified surface-distance checks at vertical boundaries and detailed finished hole/wall dimensions.
- Exact fixture contact refinement; supported stock/model geometry is vertical height-field scope.
- A stitched watertight stock mesh. Current STL is the union of adaptive upper-bound cell columns,
  including internal faces. Exactly coplanar neighbors are merged without resampling; exports are
  bounded to 40,000 columns / 128 MiB and fail explicitly above that. It is a diagnostic approximation.
- Browser timing acceptance for worst-case single-operation offset jobs. Background wasm jobs
  now yield between operations and bounded verification chunks; UI frames or status polls advance
  them. Native background jobs run off the UI thread. Synchronous command calls remain available.
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
programs, simulates them through the same independent verifier, compares overlapping stock-height
intervals and the exported reference STL, checks setup values and reports distance/time/volume
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
