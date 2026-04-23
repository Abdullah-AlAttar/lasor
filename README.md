# lasor

Transparent laser pointer overlay for presentations. Displays a red dot with a fading trail that follows your cursor on top of all windows.

## Usage

```
cargo run --release
```

Press **Ctrl+Space** to toggle the laser on/off.

## Features

- Transparent, click-through fullscreen overlay
- Red dot with fading trail
- Toggle with `Ctrl+Space`
- Multi-monitor support with per-monitor DPI scaling

## Configuration

Copy `lasor.toml` next to the executable and edit as needed. All fields are optional — missing fields use built-in defaults.

| Key | Default | Description |
|-----|---------|-------------|
| `dot_radius` | `10.0` | Dot head radius in logical pixels |
| `dot_color` | `[220, 30, 30]` | RGB color for dot + trail |
| `dot_alpha` | `220` | Dot head opacity (0–255) |
| `dot_stroke_width` | `2.0` | White border width |
| `dot_stroke_alpha` | `180` | White border opacity (0–255) |
| `trail_duration_secs` | `1.8` | How long trail points stay visible |
| `trail_max_radius` | `12.0` | Trail blob max radius near head |
| `trail_min_radius` | `3.0` | Trail blob min radius at tail |
| `trail_alpha_max` | `180` | Trail max opacity (0–255) |

Example — bigger green pointer with a longer trail:
```toml
dot_radius = 14.0
dot_color = [30, 200, 60]
trail_duration_secs = 2.5
```

## Requirements

- Rust 1.80+
- Windows, macOS, or Linux (X11)

## Build

```
cargo build --release
```

Binary output: `target/release/lasor.exe` (Windows) / `target/release/lasor` (macOS/Linux)
