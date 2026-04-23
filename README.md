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

## Requirements

- Rust 1.80+
- Windows, macOS, or Linux (X11)

## Build

```
cargo build --release
```

Binary output: `target/release/lasor.exe` (Windows) / `target/release/lasor` (macOS/Linux)
