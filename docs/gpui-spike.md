# GPUI + tray

## Run window

```bash
cargo run -p opendrive-app --features gpui -- ui
```

Buttons: Add demo account, Pause all, Resume all (via `AppCommand` / `Bridge`).

## Run tray

```bash
cargo run -p opendrive-app --features tray -- tray
```

Uses `tray-icon` + `opendrive_platform::default_menu()`.

## Combined (later)

Run both under one process: GPUI owns window; tray on a thread or shared event loop.

## Pin

- `gpui = "0.2.2"`
- `tray-icon = "0.19"`
