# Repository naming migration

The product boundary is MoErgo, not Glove80. The repositories were renamed:

- `glove80-rmk` → `moergo-rmk`
- `glove80-config` → `moergo-config`

Shared firmware lives in the
`moergo-rmk` crate, both boards are parity-tested, and `moergo-control` is the
primary local entry point. Historical Glove80 names remain where they identify
that particular board or provide compatibility.

GitHub redirects the old clone and web URLs. Maintained submodules, flake
inputs, local remotes, CI paths, links, and checkout documentation use the new
names directly.

Crate and artifact names that identify one board—such as `glove80-rmk`,
`go60-rmk`, and their UF2 files—remain board-specific. The old
`glove80-control` wrapper and build-variable aliases remain available for
compatibility.
