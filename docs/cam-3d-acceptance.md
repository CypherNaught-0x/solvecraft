# Adaptive/3D implementation evidence

The first milling PR, [#1682](https://github.com/bherbruck/solidcraft/pull/1682),
merged after all enabled CI checks passed, then was reverted in
[#1715](https://github.com/bherbruck/solidcraft/pull/1715) as out of upstream scope.
This retained development arc is based on
upstream `4a4911b4`. It adds ball tools and bounded X-parallel finishing, with
independent swept-stock, fixture and finished-surface checks. Adaptive planning,
complete 3D roughing/rest machining and constant-Z finishing remain unfinished.

## Analytic and fault checks

The local analytic harness passes 14 original fixtures and 19 operation stages,
including the existing milling cases and five new parallel-finishing solids.
Each stage includes generated motions, strict post round-trip interpretation,
stock/clearance and dimensional checks, independent analytic probes, and a
validated remaining-stock mesh. Tolerances remain 0.01 mm for dimensions,
0.001 mm for posting/interpolation and 0.0025 mm for verification uncertainty.

| New fixture | Stock cells | Charged verification work | Result |
| --- | ---: | ---: | --- |
| Plane | 304 | 533,375 | Pass |
| Sloped plane | 4,774 | 8,243,790 | Pass |
| Shallow dome | 1,666 | 3,036,355 | Pass |
| Shallow bowl | 1,540 | 3,896,511 | Pass |
| Shallow saddle | 1,618 | 4,047,657 | Pass |

These small fixtures establish numerical cases, not large-part performance.
See [their original recipes](cam-3d-fixtures.md). The bowl and saddle initially
failed stock export: adaptive upper bounds created non-manifold checkerboard
contacts. Local cell refinement now resolves them without increasing the
0.00001 mm conservative export-fill limit. The regression tests verify both
machined dimensions and the exported mesh.

Nine independent engagement tests pass, covering analytic side immersion,
full slots, separated engaged arcs, an intervening island, uncertainty,
transactional cancellation, preceding removal and NC-line-linked overloads.
This verifier is a foundation; it does not authorize adaptive posting by itself.
Four protected-slice tests cover roughing geometry and allowances. Slice tests
do not establish a complete roughing strategy.

The command test passes generation, simulation, posting, save/reopen and stale
rejection after a tool edit. The visible-editor test now passes all five strategies,
including parallel finishing. A native original ramp also passes generation, simulation and posting, with a
reviewed 2800×1900 Manufacture screenshot showing Pass/Pass. Final focused checks
pass 19 CAM command tests and 11 manufacturing/snapshot tests, including legacy
drilled bodies. Workspace Clippy with warnings denied and the updated CAM/engine
Wasm check pass. The broad local gate
passed all other test binaries and the 12-crate Wasm check, but the engine suite
reported three non-CAM robustness failures (seeds 20, 55 and 71). Seed 20 also fails with the same Boolean Cut error in the retained engine test
binary built before this feature arc, including with a single worker. The full
local gate is not recorded as passing, and no golden expectations were changed.
The separately run offline-browser gate passes.

For parallel targets, CAD input preparation canonicalizes shared coordinates only
within a coordinate-scaled
roundoff bound, capped at 1/128 of the verifier uncertainty budget, before hashing.
Generation and independent verification consume this same input; reference STL
validation stays strict. Ambiguous associations and collapsed faces are rejected.
The cutting length must span original stock top to requested bottom. A lower
operation Top cannot justify a short tool without verified preceding removal;
stock-dependent short-tool reach remains unsupported.

## Local Fusion comparison

An original 1×1×5 mm plate with 1×1×6 mm stock was recreated locally in Fusion
2705.1.30. The captured parallel operation uses a 2 mm ball tool, 0.1 mm maximum
stepover, +X cutting rows, 300/100 mm/min cutting/entry feeds and 6000 rpm.
NC Program posting, setup HTML and source STEP are captured. Independent STEP
import confirms one solid, six planar faces, the expected bounds and 5 mm³ volume.
Actual setup, tool, operation and post properties are reconciled and hashed.

The strict NC interpreter and independent verifier pass the Fusion program's
stock clearance and finished dimensions. The user exported final stock at maximum
Stock Accuracy and confirmed 100% completion with Collisions: None. Its numeric
resolution remains explicitly unreported. All three independent stock comparisons
pass: SolidCraft NC versus Fusion NC (maximum conservative bound 0.0089601 mm),
SolidCraft NC versus Fusion STL (0.0081746 mm), and Fusion NC versus its STL
(0.0067762 mm). Input reconciliation, setup values and artifact hashes also pass.

The `adaptive-3d` oracle scope currently reports **1/4 complete passing packages**.
It requires adaptive, roughing-3d, parallel and constant-z separately. Missing
strategies and artifacts remain missing; the existing milling corpus cannot
satisfy this gate. This establishes the recorded parallel-plate comparison only, not full adaptive/3D
parity or physical controller qualification. All Fusion-produced files and derived reports remain ignored locally.
