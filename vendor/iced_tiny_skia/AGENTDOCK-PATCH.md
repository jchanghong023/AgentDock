# AgentDock patch

Based on the locked crates.io `iced_tiny_skia` 0.14.1 package (MIT license).
No dependency version or platform backend changes.

`src/window/compositor.rs`: after normal damage grouping, merge more than eight
regions into their bounding rectangle. The software renderer traverses all layers
and updates clip masks for each region. OMP `/model` produced 591 grouped regions
and a 1456 ms software draw on the Windows test machine. A bounded region count
prevents this multiplication of work, while keeping small cursor/line updates
precise. The merged rectangle contains all original damage, including disjoint
regions. Its intervening pixels are repainted normally, not copied from stale data.

The same conservative repaint behavior applies to Windows and X11; no GPU,
new system API, or change to the existing winit compatibility patch is required.
