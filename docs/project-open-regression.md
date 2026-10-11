# HardEdgeBasin opening regression, 2026-10-11

The main isolated workspace starts from upstream `4c4a044`. HardEdgeBasin has
58 features and produces 14 bodies. Parsing/repair costs roughly 3 ms.

| Build | Evaluation | Feature errors | Display triangles |
|---|---:|---:|---:|
| Upstream main | 1.8 s | 8 unsupported fillets | 11,180 |
| Installed development binary | 89.5 s | 0 | 11,700 |
| Clean rebuild including fillet commit `c0b1c7f` | 23.0 s | 0 | 11,700 |
| Same fillet source with redundant fitting work removed | 21.5 s | 0 | 11,700 |

These `bench-open` runs were on an M-series Mac under varying build load. They
localize the cost, rather than provide stable microbenchmark statistics. Upstream
rejects endpoint fillets 102–109, so its quick opening is not equivalent output.
Most of the installed binary's slowdown has already been addressed by the latest
fillet changes; the additional optimization is modest on that newer source.

The separate comparison branch reuses the existing sparse spline evaluator for
boundary validation, computes each quadrature grid once, and skips construction
of collocation constraints that transition fits never use. It preserves the
position/tangency checks and control-hull containment certificate. Those changes
are not mixed into the upstream-based F3D branch.

## Regression coverage

```sh
cargo test --profile ci -p solvecraft-cli --test open_performance -- --nocapture
```

The upstream-only test used a 20-second deadline and pinned eight unsupported
feature IDs. With endpoint fillet support, the test runs `doc.open` and display
preparation in a child with a 60-second deadline (excluding compilation). It
checks saved/path state, parameter errors, all 58 evaluated features with zero
errors, and 14 bodies with
nonempty display meshes, zero recomputation on refresh, shared mesh allocations,
and unchanged source bytes. The native fixture is under `tests/performance`,
outside the historical format corpus that mutates every fixture repeatedly.

The upstream test completed in 0.90 s on a later run; that variation is not an
additional claimed optimization. When fillet support lands, change the expected
errors and remeasure the budget, rather than suppressing features or accepting
new failures. The comparison branch requires zero errors with a 60-second budget;
its end-to-end test passed in 33.3 s alongside other builds/geometry tests.

The comparison's nine setback tests passed, and 14/15 endpoint tests passed.
`endpoint_section_cannot_cross_a_hole` fails in its cylinder-cut setup on both
clean `c0b1c7f` and optimized code. This existing geometry failure is independent
of the performance patch. Targeted Clippy checks passed on both workspaces.

## Integrated validation

The performance workspace includes upstream `a094fa4` and the supported fillet
implementation. The isolated opening test passed in 25.34 seconds for open plus
display, with zero feature errors, 14 bodies and 11,700 triangles. This is one
measurement, not a controlled speedup comparison against the earlier runs.

`cargo xtask test` and `ci` run binaries named `*_performance` after all functional
test binaries and side jobs finish, so their timing budgets do not compete with
the same runner's kernel tests or compiler jobs. Geometry acceptance tolerances,
boundary sampling density and the containment certificate are unchanged.

Full CI passed formatting, attribution/licence/layer checks, workspace Clippy,
WASM, and all functional binaries (211 kernel, 548 engine, 261 UI tests). Its
last performance binary could not start because the workspace's build-output
directory had been removed during the run. Rebuilding and rerunning only that
gate passed in 52.22 seconds under concurrent machine load, with zero feature
errors, 14 bodies and 11,726 display triangles. The mesh count varies between
runs; the test checks nonempty meshes and complete feature evaluation rather
than an exact triangulation. These timings must not be read as a controlled
speedup comparison.

The existing endpoint-hole test now uses a planar square-hole tool, preserving
its intended containment assertion while avoiding the unrelated cylinder-cut
setup failure. The focused test passed, followed by the complete kernel suite.
