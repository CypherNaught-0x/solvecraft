# Small editable history fixtures

These 108 designs and their JSON command recipes are original SolidCraft test
data. They were authored through SolidCraft commands; no Fusion-produced file is
included. Each `.solidcraft` can be opened normally. Each `.json` contains its
recipe, independent expectations and optional edits.

| Fixture | Features checked | Analytic volume (mm³) |
|---|---|---:|
| circle-extrude | Driving diameter, height expression | 5040π |
| user-parameters | User/model roles, comments, favorites, dependent height | 4000π |
| negative-extrude | Signed height, downward bounds | 5040π |
| placed-circle | Offset local center, explicit XZ placement | 324π |
| plane-xy | Offset plane, parent edits and negative placement | 1500π |
| plane-yz | YZ plane offset toward +X, parameter and feature edits | 1500π |
| plane-xz | XZ plane offset toward +Y, parent edits | 1500π |
| plane-chained | Two dependent planes, parameter and feature edits | 1500π |
| suppressed-extrude | Suppression and unsuppression | 0 → 5040π |
| new_body-extrude | Two independent bodies | 5340π |
| join-extrude | Contained cylinder joined into base | 5040π |
| cut-extrude | Blind circular pocket | 4740π |
| intersect-extrude | Intersection with smaller cylinder | 300π |
| multiple-circles | Two selected regions, separate bodies, diameter/height edits | 6000π |
| multiple-rings | Two regions with separate holes, diameter/height edits | 4500π |
| partial-lens | Overlapping circles, exact arc pieces, diameter/height edits | 12 × lens area |
| partial-left-crescent | Left circle minus lens, opposite arc traversal | 12 × (900π − lens area) |
| partial-right-crescent | Right circle minus lens, diameter/height edits | 12 × (900π − lens area) |
| partial-left-rectangle | Divider middle piece, source width and height edits | 9600 |
| partial-right-rectangle | Opposite divider traversal, width and height edits | 19200 |
| offset-circle-ring | Associative offset distance, source diameter and height edits | 3300π |
| annulus-extrude | Exact outer/hole boundaries, both diameter edits | 9600π |
| arc-extrude | Shared arc/chord endpoints, radius and height edits | 1350π |
| rectangle-extrude | Width/depth dimensions, dependent expression | 10200 |
| projected-circle | Source sketch link, diameter edits, extrusion | 5040π |
| projected-body-edge | Persistent edge of one of two overlapping bodies, large edits | 7920π |
| projected-body-circle | Both named circular boundary edges retained as one circle | 7920π |
| centerline-extrude | Axis crossing a rectangle stays out of profiles, dimension edits | 10200 |
| aligned-dimension | Width and diagonal distance, dependent side and bounds | 12000 |
| rectangle-negative | Rectangular prism with downward height | 10200 |
| reordered-sketch | Feature order differs from object ID order | 5340π |
| projected-body-rectangle | Persistent shared corners, width/depth/source-height edits | 33000 |
| extrusion-start-offset | Signed start offset and dependent height expression | 10200 |
| symmetric-cylinders | Center symmetry, equal radii, diameter/height edits | 6000π |
| offset-dimension-prism | Parallel-line distance, width/depth/height edits | 10200 |
| line-midpoint-prism | Shared line centers, moving reference, width/depth/height edits | 10200 |
| fixed-arc-support | Fixed center/radius with moving endpoint, height edits | 4500(acos(1/3) − √8/3) |
| angular-prism | Driving angle, supplementary angles, height edits | 10200 |

The fixed arc support uses radius 30 mm, a fixed start at (30, 0), and an endpoint
whose X coordinate changes from 10 to 20 or 5 mm. Its segment volume is
r²(θ − sin θ)h/2, where θ = acos(endpoint_x/r). Endpoint edits preserve the
center, radius and start while changing the segment bounds and volume.

The line-midpoint prism shares the bottom edge center with an 80 mm construction
line through a free midpoint. Width and depth edits change volume; moving the
reference line changes bounds while preserving volume. Both behaviors must
survive reopening and undo.

The annulus uses radii 30 and 10 mm with height 12 mm, giving π(R² − r²)h.
The semicircular segment uses radius 15 mm and height 12 mm, giving πr²h/2.

The body-edge fixture has three independent overlapping cylinders, all with
radius 12 mm and heights 35, 11 and 9 mm. The last circle projects the top edge
of the first extrusion, selecting both semicircular boundary edges. Their summed
volume is πr²(35 + 11 + 9). Increasing
diameter to 120 mm and reducing the first height to 5 mm exercise persistent
edge selection beyond the original position-based search tolerance.
The `projected-body-circle` variant uses `sketch.project` with `circle: true`,
and additionally verifies that the projection remains one curve and one link.

The rectangular prisms are 30 × 20 × 17 mm; expectations use width × depth × height.
The aligned-dimension prism has width 30 mm, diagonal 50 mm and height 10 mm.
Its depth follows √(diagonal² − width²); diagonal and width edits check both
volume and bounds, including after reopening and undo.

The plane fixtures use radius 10 mm and height 15 mm. Editing their parent plane
changes world bounds while preserving volume. Bounds are checked after each edit,
reopening and undo, so a frozen sketch placement cannot pass on volume alone.

The base cylinder has radius 12 mm and height 35 mm. The second cylinder has
radius 5 mm and height 12 mm. Expectations use πr²h and sum/difference, not output
recorded from the implementation. The placed cylinder has radius 6 mm, height
9 mm and local center (7, -4); its local Y axis points toward world -Z.

`editable_history.rs` replays each recipe, compares the saved document, opens it
through `doc.open`, checks feature order, body count, aggregate world bounds and
volume, then checks edits, serialization/reopening and undo. Curved-volume checks
use the existing oracle tolerance of 0.1% because `inspect.measure` integrates
tessellated faces; world bounds use 0.00001 mm.

```sh
cargo test --profile ci -p solidcraft-cli --test editable_history
```

To deliberately regenerate these contemporary fixtures after changing a recipe:

```sh
cargo test --profile ci -p solidcraft-cli --test editable_history write_original_history_fixtures -- --ignored
```

These are native SolidCraft history regression tests. The separate F3D tests
check native record decoding against local Fusion API controls. Passing these
fixtures does **not** establish general F3D editable-history support.

The offset ring uses an outer radius of 30 mm and an inward gap of 5 mm,
with height 12 mm: π(R² − (R − gap)²)h. The gap expression and source
diameter remain editable; each change is checked after reopening and undo.

The multiple-region fixtures extrude circles of radii 10 and 20 mm by 12 mm.
The ring variant has holes of radii 5 and 10 mm, giving
π((10² − 5²) + (20² − 10²))12 = 4500π mm³. Diameter and height edits
check both bodies and their bounds through reopening and undo.

The partial-circle fixtures use radii 30 mm and center separation 40 mm. Their
baseline lens area is `1800 × acos(2/3) − 400 × sqrt(5)` mm². The diameter edit
uses the general two-circle intersection formula with radii 35 and 30 mm;
expectations are analytic values, not captured kernel measurements.

Five `pieces-*` fixtures additionally cover a rotated-seam lens and crescent, a
quarter-circle with cuts exactly on its seam, and both regions of a split
semicircle (including a rotated source arc). Their source radius/diameter and
height edits survive reopening and undo. Quarter areas are πr²/4. The smaller
semicircle region has area `(r² acos(x/r) − x sqrt(r² − x²))/2`, with divider
x = 10 mm; the larger region is the semicircle area minus that value.

Three `loft-*` fixtures add circular and rectangular transitions between two
sketches, including reversed section order. Source dimensions and section-plane
height stay editable through reopening and undo. The circular frustum uses
πh(r₀² + r₀r₁ + r₁²)/3. The rectangular transition uses
h(2w₀d₀ + w₀d₁ + w₁d₀ + 2w₁d₁)/6; both formulas are independent of the kernel.

Three `loft-face-*` fixtures use the top or bottom cap of a 20 mm diameter,
10 mm tall extrusion as a persistent section. The other section is a 40 mm
diameter circle 30 mm away. NewBody and Join controls have the same analytic
volume, 8000π mm³, with different body counts. Independent source diameter,
parent height and section-plane edits check that the loft follows the original
face after regeneration, saving and undo. The bottom-face control also guards
against circular loft winding being lost between oppositely oriented planes.

The three `loft-face-rectangle-*` fixtures add the corresponding rectangular
cases. A 10 × 20 × 10 mm pedestal contributes 2000 mm³; its loft to a 20 × 40 mm
section 30 mm away contributes 14000 mm³, using the rectangular transition
formula above. Width, parent-height and plane-height edits check analytic volume
and bounds independently, including the downward loft from the bottom face.


Three `loft-three-circle-*` fixtures use radii 7, 16 and 11 mm on planes at
0, 23 and 71 mm. Forward and reversed circular-chord lofts have the same volume;
the legacy uniform fixture intentionally differs. Expected volumes are analytic
integrals of `pi * r(u)^2 * z'(u)` for the quadratic interpolants, not values
captured from either CAD application's tessellation. Each recipe checks diameter
and middle-plane edits independently, then reopen and undo. These fixtures bring
the suite to 55 original saved designs.


`loft-face-joined-side` adds a side-face attachment after two same-profile
extrusions, the second a Join. Its footprint is 10 x 20 mm and its final height
10 mm. A loft from the +X face to a 20 x 30 mm rectangle 30 mm away contributes
12000 mm³; the pedestal contributes 2000 mm³. Width, Join-height and destination-
plane edits check volumes 13000, 16500 and 18000 mm³ and exact bounding boxes.
The saved face certificate retains both contributing operations. There are now
56 original saved designs at that milestone.

`rotated-offset-sketch` is a 10 x 20 x 5 mm prism on an offset YZ plane whose
sketch axes are rotated -90 degrees about X. Its 1000 mm³ volume becomes 1500
when the width changes to 15 mm, or 1600 when the extrusion changes to 8 mm.
Moving the support plane from 20 to 30 mm retains volume and translates the
exact bounds. Reopen and undo preserve all three dependencies. This brings the
suite to 57 original saved designs.


`loft-face-mixed-sketches` uses the same analytic dimensions as the joined-side
fixture, with the second extrusion coming from a separate sketch. Both source
sketches depend on the same width parameter; the saved certificate retains their
distinct feature and curve identities through edits, reopen and undo.

`loft-face-polygon-side` extrudes an eight-line stepped footprint of area 800 mm²
by 10 mm. Its outer +X side is a 10 x 10 mm rectangle at X = 40 mm. A loft to a
30 x 40 mm section at X = 60 mm contributes 11000 mm³, giving 19000 mm³ total.
Increasing the pedestal height to 15 mm gives 23833⅓ mm³; moving the destination
to X = 70 mm gives 24500 mm³. These independent expectations use the rectangular
transition formula above and include aggregate bounds. There are 59 saved designs.


`loft-join-mixed-sketches` and `loft-join-polygon-side` use the same independently
calculated dimensions and volumes as their two-body counterparts, but fuse the
transition into the existing body. They must remain one body after width, parent
height and destination-plane edits, reopening and undo. The suite contains 61
original saved designs.


`loft-serial-join` and `loft-triple-join` extend the rectangular joined pedestal
with one or two additional transitions, each 20 mm tall. The successive sections
are 20 x 40, 30 x 50 and 40 x 60 mm. Their added analytic volumes are 22666⅔ and
38666⅔ mm³, giving totals 38666⅔ and 77333⅓ mm³. The chained planes follow edits
to the first destination height. Source width, pedestal height and destination
height edits check volume, exact bounds and one body through reopen and undo.
The suite contains 63 original saved designs.


`extrusion-to-cap`, `extrusion-to-bottom` and `extrusion-to-plane` cut a 10 x 10 mm
pocket into a 40 x 30 x 30 mm block, ending 5 mm short of an exact face or a live
construction plane. Cap/bottom volumes are 33500 mm³; the plane lies 10 mm before
the cap and gives 34500 mm³. Height, pocket-width and endpoint-offset edits use
`1200 * height - 10 * width * (height - plane_gap + offset)` and exact bounds.
All cases retain one body through reopen and undo. There are 68 original models.
The `to_plane` command input accepts a construction-plane name or `{face:[x,y,z]}`;
face selection is saved as a certified attachment, not a frozen point.

`extrusion-travel-cap` and `extrusion-travel-negative-cap` retain the extrusion's
logical end rather than its initial low/high cap. Reversing the parent height
from 30 to -40 mm moves the pocket to the opposite side of XY. Both models check
44500 mm³ after reversal, plus independent width and endpoint-offset edits,
world bounds, reopening and undo.

`mirror-body-origin`, `mirror-body-plane` and `mirror-body-xz` reflect a cylinder
of radius 10 mm and height 30 mm as a separate body. Their total volume is
2πr²h = 6000π mm³; diameter and height edits give 13500π and 8000π respectively.
The offset plane moves from Z = 50 to -20 mm, moving the reflected cylinder from
Z = 70..100 to -70..-40 without changing volume. The XZ case reflects a source
centered at Y = 20 to Y = -20. Suppression retains only the original cylinder.
Every case checks exact bounds, body count, volume, reopening and undo.

`mirror-feature-new`, `mirror-feature-join` and `mirror-feature-cut` reflect an
extrusion recipe across a live YZ offset plane. A circle with radius 5 mm, centered
at X = 20 mm, creates a 30 mm extrusion or a 10 mm deep cut. The New Body case
contains two cylinders with volume 1500π mm³. Join and Cut operate on an
80 x 60 x 20 mm block, giving 96000 + 500π and 96000 − 500π mm³ respectively.
Independent diameter, extrusion distance, plane offset and suppression edits
check analytic volume, body count and bounds through reopening and undo.
The suite now contains 74 original saved designs.

Six `combine-*` designs use two 20 × 20 × 20 mm blocks overlapping by 10 mm
along X. Their volumes are 8000 mm³ each and their intersection is 4000 mm³:
Join gives 12000 mm³, Cut and Intersect give 4000 mm³. Keeping the tool adds
8000 mm³ and retains a second body. Independent target width and both extrusion
height edits recalculate the intersection as overlap-width × 20 × min(heights).
Suppression restores both source bodies. Every case checks exact world bounds,
body count, volume, reopening and undo. These bring the suite to 80 saved designs.

Twenty-four `combine-source-*` designs extend the cube controls with a prior
10 mm extension, a 10 × 10 × 10 mm pocket, or a reflection through X = 15 mm.
Join/Cut/Intersect volumes are 12000/4000/8000 mm³ for the extended source,
11500/3500/3500 mm³ for the pocketed source, and 12000/4000/4000 mm³ for
mirrored sources. Keeping the tool adds 8000 mm³. Swapping the mirror into
the target role moves the Cut result to X = 20..30 mm. Source-height edits,
mirror-plane moves to X = 17 mm, and suppression are checked analytically,
including exact bounds, reopening and undo. There are 104 saved designs.

Four `fillet-*` examples round a vertical edge, the opposite vertical pair, or
one top/bottom edge of a 20 × 30 × 40 mm extrusion. Their volume is
`width × depth × height − (1 − π/4) × radius² × selected_edge_length`.
Radius, source height, width and suppression edits check volume and bounds
through reopening and undo. The extra volume tolerance scales with removed
material, so a wrong edge cannot hide within the much larger block volume.
There are 108 original saved designs.
