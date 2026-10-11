# Small editable history fixtures

These twenty designs and their JSON command recipes are original SolveCraft test
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
| annulus-extrude | Exact outer/hole boundaries, both diameter edits | 9600π |
| arc-extrude | Shared arc/chord endpoints, radius and height edits | 1350π |
| rectangle-extrude | Width/depth dimensions, dependent expression | 10200 |
| projected-circle | Source sketch link, diameter edits, extrusion | 5040π |
| centerline-extrude | Axis crossing a rectangle stays out of profiles, dimension edits | 10200 |
| aligned-dimension | Width and diagonal distance, dependent side and bounds | 12000 |
| rectangle-negative | Rectangular prism with downward height | 10200 |
| reordered-sketch | Feature order differs from object ID order | 5340π |

The annulus uses radii 30 and 10 mm with height 12 mm, giving π(R² − r²)h.
The semicircular segment uses radius 15 mm and height 12 mm, giving πr²h/2.

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
