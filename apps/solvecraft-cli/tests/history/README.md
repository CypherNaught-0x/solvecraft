# Small editable history fixtures

These forty-six designs and their JSON command recipes are original SolveCraft test
data. They were authored through SolveCraft commands; no Fusion-produced file is
included. Each `.solvecraft` can be opened normally. Each `.json` contains its
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
cargo test --profile ci -p solvecraft-cli --test editable_history
```

To deliberately regenerate these contemporary fixtures after changing a recipe:

```sh
cargo test --profile ci -p solvecraft-cli --test editable_history write_original_history_fixtures -- --ignored
```

These are native SolveCraft history regression tests. The separate F3D tests
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
56 original saved designs.
