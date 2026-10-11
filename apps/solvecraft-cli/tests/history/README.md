# Small editable history fixtures

These thirty designs and their JSON command recipes are original SolveCraft test
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
| plane-xz | XZ plane offset toward +Y, parent edits | 1500π |
| plane-chained | Two dependent planes, parameter and feature edits | 1500π |
| suppressed-extrude | Suppression and unsuppression | 0 → 5040π |
| new_body-extrude | Two independent bodies | 5340π |
| join-extrude | Contained cylinder joined into base | 5040π |
| cut-extrude | Blind circular pocket | 4740π |
| intersect-extrude | Intersection with smaller cylinder | 300π |
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
