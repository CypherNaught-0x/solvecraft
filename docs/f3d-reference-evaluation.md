# F3D reference evaluation — 2026-10-13

The user corpus contains 17 native designs: four standalone F3D files and thirteen
F3D members of the two supplied ZIP packs. The small controlled feature tests
are not a substitute for opening these complete designs.

## References

| Project | Revision reviewed | Use and limitation |
|---|---|---|
| [cadmpeg](https://github.com/cadmpeg/cadmpeg) (Apache-2.0) | `8c19a6021ef70fe5134c9d4d08fb631725510c16`, reports version 0.6.1 | Executed the F3D decoder on all 17 designs. Public format notes and typed native records provide useful evidence. Neutral feature definitions still contain unresolved selections; successful decoding does not prove editable regeneration. |
| [ezf3d](https://github.com/AlexSabaka/ezf3d) (MIT) | `97fe90ab8c39c3978fa6354f4a21c5ebdbc38c6e` | Reviewed documentation and dependency metadata. Useful independent archive, parameter, timeline and B-rep reader. Its own roadmap leaves general feature-graph transpilation unfinished. Not executed in this evaluation. |
| [f3z-to-solidcraft](https://github.com/queball1999/f3z-to-solidcraft) (MIT) | `391ef19e5d85ef124c9c21836cf94dd75db263fa` | Reviewed its documented translation approach. It explicitly infers some dimensions and selections, skips unsupported operations, and reports that no design fully matches yet. Not executed; those fallbacks are not suitable as a silent history-preserving import path. |

The initial evaluation added no runtime dependency. The subsequent explicit
recovery modes now link cadmpeg 0.6.1 on desktop for native body conversion.
Native archives, full decoded models, fidelity sidecars and measurements stay
outside the repository.

## Measured decoder coverage

Built cadmpeg with only its `f3d` feature and used `dump --report` with its default
desktop resource limits. Sixteen files produced geometry. Gridfinity stopped at
the 16,000,000 collection-item limit while framing SAB tokens. Limits were not
disabled.

| Designs | Decoder result |
|---|---|
| Minimal; Crescent, Eye, Narrow, Square, Style 6 | Geometry decoded, no decode loss reported. |
| Adapter, R36S, modular clamp; Round, Style 2, Style 3, Style 4, Style 5, Style 7, style-1 | Geometry decoded, design-intent warnings remain. These include native-only feature definitions, unresolved profile/face/edge/body selections, and unresolved dimensions or parameter ownership. |
| Gridfinity | Resource-limit refusal; no decoded model. |

Additional `check` runs on the decoded CADIR passed for the adapter, Crescent,
Eye, Narrow, Square and Style 6. Minimal reported an ACT document-root link error.
R36S reported thirteen errors involving pcurves, procedural support and an ASM
history chain. These checks validate the decoded representation; they do not
compare a regenerated SolidCraft history with the source design.

For example, cadmpeg emits 47 neutral feature entries for the adapter's 49 native
timeline entries and 52 for R36S's 53. Entry-count differences and native operands
must be resolved before using the neutral graph as an authoritative editable
timeline. Do not treat the decoder's exit code as a fidelity verdict.

## Changes supported by the evidence

The public WorkPoint grammar identifies `boundingboxIndex` as unused for a
two-edge intersection. SolidCraft previously required its bytes to be zero,
rejecting the adapter's valid Point1 record. The reader now consumes that field
without interpreting it as a reference, supports point-data v2/v3 and the verified
legacy/modern geometry envelopes, and still checks ownership, source references,
method and cached position. Both adapter point definitions match their unchanged
modern resave. Modern native point imports still pass source-edit, volume,
position, reopen and undo checks.

Several knob sketches also have the v17 member list immediately followed by the
final parent reference. The reader previously interpreted that reference as an
extension-table count. It now accepts the exact final-reference envelope while
retaining and validating extension metadata when present.

Import errors now identify the failing timeline object, class and version;
reference failures also include the record offset or expected/actual class.

## Complete-import acceptance gate

`apps/solidcraft-cli/tests/f3d_corpus.rs` attempts every archive instead of stopping
at the first failure. It requires nonempty history, successful regeneration,
positive finite solid volume, lossless history container round-trip and stable
reopened volume. Minimal also has its independent analytic cylinder check. The
gate is opt-in because the user files are not committed and compatibility remains
incomplete. A failed import is a failed gate, not a passing expected-rejection test.

```sh
SOLIDCRAFT_F3D_CORPUS=/path/to/test_files \
SOLIDCRAFT_F3D_ADDITIONAL_CORPUS=/path/to/extracted_f3d_members \
cargo test --profile ci -p solidcraft-cli --test f3d_corpus -- --ignored --nocapture
```

The initial result is **1/17 complete imports (Minimal)**. The construction-point
fix advances the adapter to its chamfer blocker; it does not make that complete
design importable. General editable F3D support remains unfinished.

Validation of this change: 130 focused F3D tests passed, along with the native
point import/edit controls and original/resaved adapter point comparison. The
full quality gate found a byte-offset mistake in the new authored test fixture's
variable-length name; that fixture was corrected and the focused suite rerun.
All other full-gate tests passed, including the saved-history fixtures and the
opening-performance guard (1.03 seconds). Final formatting and clippy checks
passed; WASM, assets, licences and layering passed in the full gate.

Upstream was checked at `be55a49b`. This investigation keeps the existing F3D
branch's integrated baseline (`0f0bb098`); integration of the later upstream
kernel and UI changes remains separate work before a general-import release.

The most useful next integration target is cadmpeg's native, versioned record
and historical-topology decoding. Retain SolidCraft's proven expression,
constraint and feature reconstruction where it is more complete. Any shared
decoder must first satisfy bounded-input handling and browser/WASM builds, and
each mapped feature needs regeneration and edit checks. Native-only definitions,
ambiguous selections and missing timeline entries remain explicit errors in
strict mode and are listed as omissions in user-selected recovery modes.

## Explicit recovery modes

The desktop open dialog now offers editability or correctness as a preference.
Editability reconstructs supported history and omits failed features and their
dependants. If no solid can be rebuilt, it attempts native bodies while retaining
recovered sketches. Correctness tries native bodies first, then partial history.
Neither option certifies the final geometry. Import reports persist on save.

All 17 supplied archives opened and survived save/reopen in both modes in the
opt-in recovery test. Sixteen recovered solids; Gridfinity produced an empty
document with an explicit report. Tests verify stable finite display-mesh volume
on reopen and independently check Minimal's analytic cylinder volume in both
modes. This is recovery coverage, not complete editable-history coverage.

The native adapter raises the decoder's collection budget from 16M to 64M while
retaining byte, recursion and work budgets. Gridfinity then reaches cadmpeg's
4-billion work-unit budget (one requested operation accounts for about 24 billion
units). These are software limits, not detected RAM exhaustion. Cold display
meshing of the modular clamp remains slow. Full-history reconstruction and
fidelity comparisons against Fusion remain unfinished.
