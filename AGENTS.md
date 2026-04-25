# Lasor – Agent Guide

## What is this project?

**Lasor** is a lightweight screen-overlay tool for presentations and screencasts. It renders a
transparent, always-on-top window that spans all monitors and provides:

- **Laser pointer** – an animated glowing dot with a fading trail that follows the cursor.
- **Draw / annotation** – persistent freehand strokes, each with its own colour, undoable with
  `Ctrl+Z`.
- **Colour picker** – a collapsible palette popup with 8 preset colours for draw strokes.
- **Draggable toolbar** – a compact HUD that floats near the bottom-right of the primary monitor.

## Stack

| Concern | Choice |
|---------|--------|
| Language | Rust (edition 2024) |
| GUI / rendering | `eframe` 0.31 + `egui` 0.31 (wgpu backend) |
| Window management | `winit` 0.30 (via eframe) |
| Windows APIs | `windows-sys` 0.59 |
| Config | `toml` 0.8 + `serde` |

The app is **Windows-first**: multi-monitor spanning, per-monitor DPI, cursor position, and
taskbar-aware work-area detection all use raw Win32 calls. Non-Windows builds compile and run via
fallback stubs in `platform.rs`.

## Source layout

```
src/
  main.rs          Entry point. Reads virtual-desktop dimensions, configures eframe,
                   starts the event loop.
  config.rs        Config struct (loaded from lasor.toml next to the executable).
                   Falls back to built-in defaults if the file is absent or malformed.
  mode.rs          Mode enum: Idle | Laser | Draw.
  platform.rs      Win32 helpers: cursor position, per-monitor DPI scale,
                   primary-monitor work-area bottom-right corner.
                   Non-Windows stubs are in the same file behind #[cfg(not(windows))].
  app/
    mod.rs         LasorApp struct definition, new(), passthrough logic,
                   window-spanning (SetWindowPos on frame 1), and the
                   eframe::App::update() main loop.
    laser.rs       update_laser (trail interpolation + pruning),
                   paint_laser (ribbon pass + circle caps + dot head).
    draw.rs        update_draw (stroke input, Ctrl+Z undo),
                   paint_strokes (per-stroke colour rendering),
                   paint_stroke (polyline / single-dot helper).
    toolbar.rs     All toolbar UI: show_toolbar, show_color_palette_popup,
                   draw_grip_handle, toolbar_mode_buttons, toolbar_color_toggle,
                   toolbar_action_buttons, clamp_toolbar_pos.
                   Also owns DRAW_PALETTE, TOOLBAR_MARGIN_X/Y, toolbar_frame(),
                   mode_button() free functions.
```

## Key types

### `LasorApp`  (`app/mod.rs`)
Central state struct. Passed as `&mut self` to every method. Notable fields:

| Field | Type | Purpose |
|-------|------|---------|
| `mode` | `Mode` | Current operating mode |
| `trail` | `VecDeque<TrailPoint>` | Live laser trail points with timestamps |
| `strokes` | `Vec<(Vec<Pos2>, [u8;3])>` | Completed draw strokes, each with its own RGB |
| `current_stroke` | `Vec<Pos2>` | Stroke being drawn right now |
| `draw_color_rgb` | `[u8; 3]` | Currently selected draw colour (RGB, no alpha) |
| `show_color_picker` | `bool` | Whether the palette popup is expanded |
| `toolbar_pos` | `Option<Pos2>` | `None` until auto-placed on frame 2; then locked/dragged |
| `toolbar_rect` | `Rect` | Last measured toolbar bounding rect (for passthrough & popup anchor) |
| `color_toggle_rect` | `Rect` | Last measured rect of the colour toggle button (palette anchor) |

Private fields (`positioned`, `last_passthrough`) are only touched inside `app/mod.rs`.

Fields accessed by submodules (`laser.rs`, `draw.rs`, `toolbar.rs`) are `pub(crate)`.
Methods only called from `update()` are `pub(super)`; all internal helpers are private.

### `Mode`  (`mode.rs`)
```rust
pub enum Mode { Idle, Laser, Draw }
```
Drives which update and paint functions run each frame.

### `TrailPoint`  (`app/mod.rs`)
```rust
pub(crate) struct TrailPoint { pub(crate) pos: Pos2, pub(crate) time: Instant }
```
Time-stamped cursor sample used for the fading laser trail.

### `Config`  (`config.rs`)
All tunables (dot radius, colours, trail duration, draw width, alpha values).
Loaded once at startup; never mutated at runtime.

## Frame loop  (`app/mod.rs` → `eframe::App::update`)

```
position_window_once()          // one-shot SetWindowPos on frame 1 (Windows)
cursor_info()                   // Win32 cursor pos + per-monitor DPI scale
should_passthrough() → send     // click-through unless toolbar hovered or Draw mode
match mode:
  Idle  → clear trail/pos
  Laser → update_laser()
  Draw  → update_draw()
match mode → request_repaint / request_repaint_after(200ms)
CentralPanel:
  paint_strokes()               // draw annotation strokes (always visible)
  if Laser: paint_laser()       // laser trail + dot head
show_toolbar()
show_color_palette_popup()
```

## Toolbar startup position

Auto-placement lives in `show_toolbar` (`app/toolbar.rs`).  
It is a two-phase calculation driven by `toolbar_pos: Option<Pos2>`:

- **Frame 1** (`toolbar_pos == None`, size unknown): provisional position `br - (350, 100)`.
- **Frame 2** (`toolbar_pos == None`, size known): compute exact top-left so the toolbar's
  bottom-right sits `TOOLBAR_MARGIN_X` / `TOOLBAR_MARGIN_Y` px from the primary monitor's
  work-area corner, then lock `toolbar_pos = Some(p)` permanently.

To change the default position adjust the two constants at the top of `toolbar.rs`:
```rust
const TOOLBAR_MARGIN_X: f32 = 40.0;   // distance from right edge
const TOOLBAR_MARGIN_Y: f32 = 10.0;   // distance from bottom edge
```

## Colour picker

`DRAW_PALETTE` in `toolbar.rs` is the list of preset swatches. Each entry is `([r,g,b], label)`.
Adding or removing colours is done by editing that constant only.

The alpha applied to strokes comes from `cfg.draw_alpha` (set in `lasor.toml`), not from the
palette. The palette only stores RGB.

Each completed stroke captures `draw_color_rgb` at commit time, so changing colour mid-session
never re-tints old strokes.

## Mouse passthrough

The overlay window is click-through by default. `should_passthrough` returns `false` (capturing
mouse) when:
- `mode == Draw` (the whole canvas must receive paint events), or
- the cursor is within `toolbar_rect.expand(4)`.

The result is only sent to the OS when it changes, to avoid redundant round-trips.

## Platform notes

- All Win32 calls are in `platform.rs` or inside `#[cfg(windows)]` blocks in `app/mod.rs`.
- Non-Windows builds use egui's reported screen rect and pointer position as fallbacks.
- The window title is `"lasor"` – `FindWindowW` in `position_window_once` depends on this
  matching exactly.

## Build & lint

```bash
cargo build            # debug
cargo build --release  # release (opt-level 3)
cargo clippy           # must be zero warnings (clippy::all is enabled in main.rs)
```

## Coding conventions

- Follow the existing patterns in the skill files under `.agents/skills/`.
- No `get_` prefix on methods (`name()` not `get_name()`).
- `SCREAMING_SNAKE_CASE` for constants, `CamelCase` for types, `snake_case` for everything else.
- Prefer `pub(super)` for methods exposed to the parent module; `pub(crate)` for struct fields
  that submodules need to access.
- Keep clippy clean – run `cargo clippy` before finishing any task.
- Do not use `2>nul` in shell commands on this project (Windows PowerShell creates a literal
  `nul` file). Use `/dev/null` or `2>$null` instead.