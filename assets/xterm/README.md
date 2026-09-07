# Embedded xterm.js assets

Pinned npm packages: `@xterm/xterm` 6.0.0, `@xterm/addon-fit` 0.11.0,
`@xterm/addon-unicode11` 0.9.0. UMD distributions and CSS are copied unchanged
from their npm tarballs; licenses are included alongside them. No CDN, npm, or
network access is required to run AgentDock. Source: https://github.com/xtermjs/xterm.js

`terminal.js` is the application bridge. Rust sends ordered binary PTY batches;
the next batch waits for xterm's write completion callback. Input has a 1 MiB
limit and one 32 KiB write in flight per session, acknowledged after the PTY
writer finishes. Session epochs reject messages from disposed/restarted panes.

The Windows WebView2 child runs without the WebGL addon and with `--disable-gpu`.
It is restricted to bundled assets and cannot navigate to remote pages.
