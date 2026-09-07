# AgentDock X11 compatibility patch

Vendored from the crates.io `winit` 0.30.13 package, checksum
`a6755fa58a9f8350bd1e472d4c3fcc25f824ec358933bba33306d0b63df5978d`.
The upstream Apache-2.0 license is retained in LICENSE.
Cargo.toml selects this source using `[patch.crates-io]`.

Changes are limited to the Linux X11 keyboard path:

- Treat the XKEYBOARD extension as optional; never select or dispatch XKB events
  when it is absent. Propagate keyboard initialization failures as event-loop errors.
- Without XKEYBOARD, compile a local libxkbcommon keymap. Respect an explicit
  `XKB_CONFIG_ROOT`; otherwise append `/usr/share/X11/xkb` to the search paths.
  libxkbcommon's `XKB_DEFAULT_RULES/MODEL/LAYOUT/VARIANT/OPTIONS` apply.
- Use local state for synthetic events and core X11 modifier state on focus.
- Allow unavailable detectable auto-repeat and recognize core release/press pairs
  on the no-XKB path.
- Check availability of libxkbcommon-x11 before calling its infallible loader.

The fallback uses a local layout (normally evdev/pc105/US); it does not discover
the remote server's keyboard layout. Non-US layouts and non-evdev keycodes need
site-specific validation/configuration. XInput2 and other existing winit window
requirements remain. This patch does not establish CentOS ABI or Citrix support.

Regression: `python3 scripts/test_x11_keyboard.py` from the repository root on
Linux (see script prerequisites). It uses a TCP X11 proxy to hide XKEYBOARD
QueryExtension replies, tests real window creation and key events, and checks
the missing-data error. This is simulated extension absence, not a Citrix test.
