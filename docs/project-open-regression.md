# HardEdgeBasin opening regression, 2026-10-11

The main isolated workspace started from upstream `4c4a044` and was fast-forwarded
to `463441a` before final validation. HardEdgeBasin has
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

The test runs `doc.open` and display preparation in a child with a 20-second
deadline (excluding compilation). It checks saved/path state, parameter errors,
all 58 evaluated features, the exact unsupported-feature IDs, 14 bodies with
nonempty display meshes, zero recomputation on refresh, shared mesh allocations,
and unchanged source bytes. The native fixture is under `tests/performance`,
outside the historical format corpus that mutates every fixture repeatedly.
`cargo xtask test`/`ci` runs binaries named `*_performance` after all other tests
and compiler jobs finish: the first full run demonstrated that concurrent kernel,
engine and WASM compilation could exhaust this budget through CPU contention.

The upstream test completed in 0.90 s on a later run; that variation is not an
additional claimed optimization. When fillet support lands, change the expected
errors and remeasure the budget, rather than suppressing features or accepting
new failures. The comparison branch requires zero errors with a 60-second budget;
its end-to-end test passed in 33.3 s alongside other builds/geometry tests.

The comparison's nine setback tests passed, and 14/15 endpoint tests passed.
`endpoint_section_cannot_cross_a_hole` fails in its cylinder-cut setup on both
clean `c0b1c7f` and optimized code. This existing geometry failure is independent
of the performance patch. Targeted Clippy checks passed on both workspaces.

Final validation on upstream `463441a`: `cargo xtask ci` passed formatting, assets,
licences, layers, workspace Clippy, WASM and all 23 test binaries, including 521
engine, 161 kernel, 249 UI and 68 scenario tests. The isolated opening test finished
in 0.96 seconds (1.4 seconds including runner overhead). The local F3D corpus and
eight controlled Fusion export comparisons also passed after upstream integration.
