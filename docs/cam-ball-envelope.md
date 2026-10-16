# Ball-cutter verifier foundation

`solidcraft_cam::cutter_envelope::ball_sweep` is an independent, constant-work
primitive for the adaptive/3D milling milestone. It describes one straight motion
of a ball-end cutter using tool-tip coordinates. Ball tools now persist through
the command/tool-library workflow, and stock and fixture checks use their actual
lower hemisphere, cutting cylinder, shank and holder. Parallel finishing has command, visible-editor, native ramp and analytic
coverage. Existing flat-mill and drill definitions retain their shapes.

## Geometric bounds

For radius R, the ball center is R above the programmed tip. At an XY point, the
lowest swept sphere height is the minimum of

`tip_z(t) + R - sqrt(R² - distance_xy(t)²)`

over the finite portion of the segment intersecting that vertical line. For a
nonvertical XY segment, let s be distance along the segment, L its XY length,
dz its height change, u the projection of the query point, and q the transverse
distance. The cross-section radius is `a = sqrt(R² - q²)`. The convex function's
stationary point is `s = u - dz*a/hypot(L,dz)`, clamped to the segment and sphere
intersection interval. Vertical and stationary motions use their lowest tip pose.
This accounts for contact at an interior pose on a slope; nearest XY distance or
the lowest endpoint alone would give the wrong machined surface.

A rectangular stock cell can be enclosed by an XY disk of radius h. Inflating the
sphere to R+h gives a lower height bound for every point in that disk. Eroding it
to R-h gives a common covered height: the triangle inequality places every disk
point inside the real sphere at that pose. The ball center's lift stays R in both
queries. If erosion has no intersection, coverage remains unresolved and old stock
must be retained. If the inflated sphere misses, the whole cell is untouched.

The implementation adds an outward floating-point guard scaled by coordinate
magnitude. Inputs are finite and bounded by the CAM model limits. Tangencies and
partial coverage cannot become whole-cell certificates. Returned height intervals
include the guard; no caller may treat the requested cell radius as achieved error.

For constant-height and vertical motion, stock verification uses conservative
rectangle-to-segment radial extrema directly. For general chords, convexity of
the swept sphere provides tighter rectangular bounds: the four corner heights
bound the maximum, while the center's supporting tangent bounds the minimum.
Tangencies and ill-conditioned derivatives retain the enclosing-disk bound.
These refinements preserve correlation between XY position and height on slopes;
they do not increase the dimensional or verifier uncertainty budgets.

## Independent generator contact calculation

The generator's separate `generate::contact` module drops a ball onto triangle
faces, edges and vertices. It does not call the swept-removal verifier. A linear
motion is checked over its entire length by subtracting its displacement from
each target triangle, forming a convex triangular prism. Contact with all eight
prism boundary triangles bounds an intervening gouge, including a narrow peak
that endpoints and midpoint miss. Concavity of each triangle's contact surface
gives an upper chord-error bound when both endpoints contact that triangle.
Uncertified chords subdivide within explicit work/depth/motion limits.

## Qualification so far

Eight tests pass for stationary tip contact, the 3-4-5 sphere section, horizontal
and vertical sweeps, finite endcaps, reversed traversal, 45-degree slopes, parallel
pass cusp height, shrinking cell intervals, tangency, large translations, nearly
vertical motion and invalid inputs. A deliberately wrong tool radius produces a
detectable dimensional discrepancy. The sloped and cusp expectations come from
analytic equations, independently of the implementation's segment minimizer.
The initial focused run, CAM Clippy with warnings denied, and the CAM
`wasm32-unknown-unknown` check pass. Whole-workspace and machining acceptance
remain separate delivery gates.

Additional focused tests pass for document save/reopen and undo/redo, wrong-radius
NC, unsafe rapids, cancellation, resource exhaustion, analytic generator contacts,
and small plane/slope fixtures' generation/post/independent verification. The
133-test CAM unit suite and integration tests passed with rectangular sweep bounds
and whole-surface coverage certificates. Dome, bowl and saddle machining and stock
exports now pass, as does the command workflow (post, reopen and stale rejection).
Curved stock can create checkerboard contacts between conservative adaptive cells;
the verifier recomputes those cells more finely, within its work/depth limits,
instead of widening the stock export's 0.00001 mm fill allowance. The visible UI
workflow now passes for all five strategies. The broad local gate has three
non-CAM robustness failures under investigation. The original parallel plate has
a complete passing local Fusion package using the same 0.01 mm, 0.001 mm and
0.0025 mm budgets; adaptive, 3D roughing and constant-Z packages remain missing.
See [the acceptance record](cam-3d-acceptance.md).
