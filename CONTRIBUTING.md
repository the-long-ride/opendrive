# Contributing

1. Follow the design in `docs/OpenDrive-design.md`.
2. PRs require CI green (`ci-ok`).
3. Dependencies must pass `cargo deny` (permissive licenses only).
4. Keep UI-agnostic core; no GPUI in non-app crates.
