# Original fixtures for the remaining clearing milestone

These are independently authored recipes for the next implementation steps.
They are planned cases, not passing results or captured Fusion references.
All coordinates are millimetres in an identity G54 frame. Preserve the existing
0.01 / 0.001 / 0.0025 mm dimensional, post and verifier budgets.

## Stepped boss: adaptive clearing and 3D roughing

Use stock `(0,0,0)..(12,12,6)`. The target is a 12×12×2 base and a central
boss with footprint `(4,4)..(8,8)` extending from Z=2 to Z=5. Its exact volume
is `12*12*2 + 4*4*3 = 336 mm³`. Stock volume is 864 mm³. The intended final
removal is 528 mm³; local top/wall checks must accompany this volume check.

Use a 2 mm flat end mill, cutting length 20, shank diameter 2, overall length 50,
and cylindrical holder diameter 20/length 30. Declare 300 mm/min cutting feed,
100 mm/min entry feed, 6000 rpm clockwise, coolant off, clearance Z=15 and
retract Z=10. The initial tool center must enter from outside stock.

For the adaptive case, declare maximum equivalent radial immersion 0.4 mm and
maximum stepdown 1 mm. A straight side cut with R=1 has an occupied forward angle
`acos(1-0.4/1) = acos(0.6)`. A full-width slot is π radians and 2 mm immersion,
so it must fail this load setting. Corner and island transitions must remain
bounded over complete motions. Reducing feed does not turn excess geometric
immersion into a passing result. The planner must change the path or reject it.

For roughing, use separate 0.2 mm radial and 0.2 mm axial allowances. The protected
boss footprint is a square expanded by a 0.2 mm disk, including rounded outside
corners. Its area is `16 + 16*0.2 + π*0.2²`. The ideal allowance envelope inside
stock has volume `144*2.2 + 3*(16 + 16*0.2 + π*0.2²)`. This is a lower protected
envelope, not a demand that every intermediate Z layer attain that exact volume.
Specify each completed level's permitted rest stock separately.

Faults: a direct full-slot connector, omitted radial allowance, an overly deep
level, a rapid across the remaining boss, and a fixture in a previously clear
entry corridor. Diagnostics must identify the operation, motion and NC line.

## Two bosses: actual rest stock and narrow access

Keep the same stock and base. Replace the central boss with footprints
`(3,3)..(5,9)` and `(7.6,3)..(9.6,9)`, both extending from Z=2 to Z=5.
The target volume is `288 + 2*2*6*3 = 360 mm³`. The gap is 2.6 mm wide;
0.2 mm radial allowance on both sides leaves 2.2 mm cutter-access width.

First rough with a 6 mm flat mill, then request rest machining with the 2 mm tool.
The larger cutter cannot clear the gap. The second operation must consume a
fingerprinted material state reflecting actual preceding removal. Naming the
first operation or claiming its intended region is insufficient evidence.
The small cutter can fit geometrically, but its entry and engagement still need
proof: the implementation must not silently treat the uncut gap as air.

Repeat with a 1.8 mm gap: a 2 mm tool is inaccessible even before allowance.
Reverse the two operation IDs without changing their ordered program sequence,
then reverse the actual sequence. IDs must not control machining order; a real
sequence change must invalidate the rest-stock dependency and all derived paths.

## Steep surface: constant-Z finishing

Use a 10×10×1 base and a centered truncated cone from Z=1, radius 3, to Z=5,
radius 1. Its volume is `100 + (4π/3)*(3² + 3*1 + 1²)` mm³. Use the same 2 mm
ball tool as the parallel corpus. Declare the finished region explicitly as the
cone's lateral surface between Z=2 and Z=4.5; the sharp floor/cone junction is
not fully accessible to a ball cutter and must not be silently included in a
whole-part finishing claim.

On the cone interior, the target radius at height z is `3-(z-1)/2`. For ball
radius R and programmed tip height z, the tangent cutter-center radius is
`3-(z-1)/2 + R*(sqrt(5)-1)/2`. This independently derived normal-offset relation
checks contact geometry. End rings, connectors and step spacing require their
own finite-region bounds. Validate normal surface distance, not only vertical
height or total volume. Inject a wrong radius, an inward connecting chord and
missing height levels.

## Capture and acceptance

Create each recipe independently in local Fusion with the same stock, tools,
frame and declared finished/rough regions. Capture the actual available strategy
and every relevant entry/link/allowance setting; defaults do not establish input
equivalence. Use NC Program posting and Save Stock after each completed operation.
Preserve post hashes/properties, setup HTML, source geometry, simulation findings,
timestamps and Accuracy settings in ignored local packages.

Adaptive engagement requires its own independent NC certificate in addition to
the existing clearance, protected-solid and finished-region checks. Roughing
allows different intermediate stock only inside its declared permitted region.
Missing entry proofs, unavailable strategies, incomplete captures and exhausted
verification budgets remain inconclusive. The milestone gate must not pass until
all required strategies have complete, passing local reference packages.
