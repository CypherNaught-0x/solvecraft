# Manufacture interface

The setup browser provides the working context for milling. Choose a setup to see
its stock, work offset, operations and NC programs. New operations and programs use
that setup. Switching context clears the displayed job and playback so a previous
program's result cannot be mistaken for the selected program.

Operations included in the active NC program appear in execution order, with their
sequence number and tool diameter. Other operations are explicitly marked as not
included. Use **Edit program / order** to change the machining sequence through the
existing program command. Document tools remain available in a separate browser group.

Operation editors divide their settings into five categories:

| Category | Settings |
| --- | --- |
| Tool | Named tool selection, diameter/cutting-length summary, feed, plunge, spindle and coolant |
| Geometry | Stock/model scope, linked profiles or holes, editable unlinked boundaries |
| Heights | Setup clearance/retract summary, operation top/bottom and drill depth convention |
| Passes | Stepdown, stepover, allowance, tolerance, direction and peck depth |
| Linking | Supported contour/pocket leads and ramp entry |

The height illustration is a labeled schematic, not a scaled model or collision
certificate. Values are absolute millimetres in the setup coordinate frame.
Tab changes preserve the draft; **Save** executes the registered command and
**Cancel** discards it. No manufacturing data format changes are needed.

**Generate → Simulate → Post G-code** remains visible above three review pages:

- **Toolpath:** progress, validation findings, line-linked issue location and playback.
- **Display:** model/initial/final stock, fixtures, toolpath and boundary visibility.
- **Output:** reference G-code, setup sheet, path JSON, validation JSON and stock STL.

The interface disables actions with missing prerequisites and explains the next
step. Changed inputs require regeneration. Failed or inconclusive validation blocks
posting while keeping the validation report available. The engine commands retain
the authoritative freshness, clearance, dimensional and post round-trip checks.
The reference dialect remains unqualified for a physical controller. Stock display
is the verified final stock; playback does not show intermediate material removal.

Workflow grouping was informed by Autodesk's public
[operation documentation](https://help.autodesk.com/view/fusion360/ENU/?contextId=MFG-REF-2D-CONTOUR-CMD).
The UI implementation, labels, schematic and theme are independently authored;
no commercial product artwork or screenshots are included.
