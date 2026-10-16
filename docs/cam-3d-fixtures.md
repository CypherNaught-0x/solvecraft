# Original 3D milling fixtures

The first two recipes are independently authored analytic solids, implemented in
`crates/cam/src/corpus/parallel.rs`. They are shared input recipes for local Fusion
captures; their existence is not evidence of a completed reference comparison.
All dimensions below are millimetres. The small size exercises the numerical error
budget; larger-part performance and curved-surface qualification are separate gates.

Three additional original shallow patches exercise curvature in both directions:
`parallel-dome` has `Z=5-0.1((X-.5)²+(Y-.5)²)`, `parallel-bowl` has
`Z=5+0.1((X-.5)²+(Y-.5)²)`, and `parallel-saddle` has
`Z=5+0.1((X-.5)²-(Y-.5)²)`. Their 1×1 footprints, stock and operation settings
match the table below. Each uses an 8×8 grid of planar triangles with a closed
bottom and sides; its maximum interpolation difference from the analytic patch
is 0.00078125 mm. The analytic equations supply independent height expectations.
These new curved cases are under qualification, with no Fusion comparison yet.

| Value | Parallel plane | Parallel slope |
| --- | --- | --- |
| Solid footprint | X/Y 0–1 | X/Y 0–1 |
| Solid bottom | Z=0 | Z=0 |
| Solid top | Z=5 | Z=5+0.2X |
| Analytic volume | 5 mm³ | 5.1 mm³ |
| Rectangular stock | (0,0,0) to (1,1,6) | Same |
| Work frame | Identity, G54 | Same |
| Tool | 2 mm ball end mill, T1 | Same |
| Cutting/shank/overall length | 20 / 2 diameter / 50 | Same |
| Holder | 20 diameter × 30 length | Same |
| Strategy | Parallel, unidirectional +X passes (`climb=false`) | Same |
| Maximum stepover | 0.1 | Same |
| Feed / entry feed | 300 / 100 mm/min | Same |
| Spindle / coolant | 6000 rpm clockwise / off | Same |
| Top / bottom | 6 / 4.5 | Same |
| Retract / clearance | 10 / 15 | Same |
| Stock allowance | 0, vertical | Same |
| Dimensional tolerance | 0.01 | Same |

Construct the sloped solid as a closed XZ quadrilateral with vertices (0,0),
(1,0), (1,5.2), (0,5), extruded one millimetre along +Y. No imported vendor geometry
is needed. In SolidCraft, the analytic fixture supplies an oriented closed triangle
mesh with those exact vertices. In Fusion, create the same dimensions and explicitly
record the actual tool, setup and operation values rather than assuming defaults.

For a radius-R ball on the plane Z=mX+b, the interior contact tip height is
`mX+b+R(sqrt(1+m²)-1)`. A flat surface between passes spaced s has cusp height
`R-sqrt(R²-(s/2)²)`. These formulas qualify the independent verifier and contact
generator. Final height probes compare against the intended top, reserving the
declared tolerance for cusp, interpolation and numerical uncertainty.

Tool centers must extend beyond the target footprint to finish sloped boundaries.
SolidCraft extends its raster by one radius and uses Bottom Z to bound overscan
depth. Reconcile Fusion's tool-containment, contact-point boundary and machining
boundary settings against that machined region. Different lead paths or pass
ordering are acceptable; a different finished region or stock allowance is not.

Capture the complete local `parallel` package with NC Program output, post identity
and hash, post properties, source geometry, setup sheet, input reconciliation and
simulation provenance. Save final stock after 100% playback at maximum Accuracy;
record the setting and leave numeric resolution `not_reported` if unavailable.
Store all Fusion-generated files under ignored `plan/fusion/cam/` storage.
The next-milestone oracle additionally requires adaptive, roughing-3d and constant-z
packages. Its report must remain incomplete until all four are present and pass.
