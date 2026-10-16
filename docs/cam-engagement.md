# Independent engagement bounds

`solidcraft_cam::engagement::horizontal` is the first independent measurement
primitive for adaptive clearing. Its nine analytic/history/NC tests pass; it is not
yet connected to an adaptive planner or posting approval.

For a horizontal straight motion of a flat cutter, measure total occupied angle
on the forward semicircle. A full slot is π radians and one tool diameter of
equivalent radial immersion. A straight side cut of width a and radius R has
angle `acos(1-a/R)`. Report equivalent immersion as `R(1-cos(angle))`. Separate
engaged arcs add; overlapping material columns never double-count an angle.
This is a geometric engagement definition, not a force or chip-thickness model.

The input contains conservative material columns before the motion. For the
whole-motion upper bound, subtract all possible center positions from each column
and enclose the result in a rectangle. Intersect that rectangle analytically with
the cutter circumference and forward semicircle. Union the resulting angular
intervals. This includes an obstacle between endpoints and bounds every pose;
diagonal motions may overestimate because their enclosing rectangles include
extra points. Smaller motion intervals refine that uncertainty.

The lower bound is a witness at the midpoint. Erode definite material by the
declared uncertainty, intersect its columns with the circumference, and union the
occupied arcs. A forward-facing circumference point has not been cut by earlier
poses on the same straight motion. Prior motions must already be reflected in the
supplied material state; the primitive never assumes a named operation removed it.

Input and numerical uncertainty expand possible material and erode definite
material. Non-finite values, nonhorizontal/zero-length moves, cuts below the known
stock bottom, excessive local column counts and exhausted work budgets are errors.
A spatial index may omit only columns proved outside the swept cutter. Missing
or unresolved stock cannot be treated as air.

`engagement::stock::MaterialStock` tracks preceding horizontal flat-cutter sweeps
in bounded material columns. Possible capsule overlap lowers only a column's lower
height bound; only complete coverage lowers its upper bound. Motion uncertainty
expands possible removal and erodes definite removal. Updates commit atomically
after incremental work, so cancelling a future preserves the previous state and
revision. Queries retain every potentially engaged column and reject excessive
local coverage rather than dropping material. This independent history is not yet
wired into posting or the adaptive planner.

`engagement::program::verify` consumes the independent modal NC reader's output
and tracks material between horizontal cuts. It reports the actual NC line and,
only after a checked post round trip, the originating operation/block. A definite
excess load fails; an upper bound straddling the maximum is inconclusive. Entries
with possible stock contact, non-flat cutting tools, unsupported rapids and work
exhaustion cannot pass. This report supplements the existing dimensional and
clearance verifier; an engagement-only pass is never enough to post.

The parsed reference NC has exact constant Z on horizontal moves, including arcs.
Their chord-error reserve is lateral. The history preserves that distinction so a
preceding cut at the same Z does not leave a fictitious vertical layer. The general
material primitive also supports explicit XYZ uncertainty when that is required.
Neither analysis models physical machine deflection, runout or chip forces.

Hand-calculated tests cover 0.1/0.4/1/2 mm side immersion for R=1, reversed motion,
two separated cutting arcs, duplicate columns, a thin intervening island and
uncertain stock. History tests cover preceding removal, unresolved edge columns,
diagonal endcaps, cancellation, actual NC-line provenance and a repeated cut that
sees preceding removal. The next integration needs separate declared entry rules and
whole-job
engagement certificates. Adaptive clearing must adapt its moves to these bounds;
ordinary offset pocketing alone does not satisfy this milestone.
