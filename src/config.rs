use serde::Deserialize;

/// All user-facing tunables loaded from `lasor.toml`.
#[derive(Deserialize)]
#[serde(default)]
pub struct Config {
    // ── Laser pointer ──────────────────────────────────────────────────────
    /// Radius of the main dot in logical pixels (before DPI scaling).
    pub dot_radius: f32,
    /// RGB color of the dot and trail [r, g, b], each 0-255.
    pub dot_color: [u8; 3],
    /// Opacity of the dot head, 0-255.
    pub dot_alpha: u8,
    /// Width of the white border stroke around the dot head.
    pub dot_stroke_width: f32,
    /// Opacity of the dot border, 0-255.
    pub dot_stroke_alpha: u8,
    /// How long (seconds) a trail point remains visible.
    pub trail_duration_secs: f32,
    /// Maximum radius of trail blobs (at the head end).
    pub trail_max_radius: f32,
    /// Minimum radius of trail blobs (at the tail end).
    pub trail_min_radius: f32,
    /// Maximum opacity of trail blobs, 0-255.
    pub trail_alpha_max: u8,

    // ── Draw / annotation ──────────────────────────────────────────────────
    /// RGB color of annotation strokes [r, g, b], each 0-255.
    pub draw_color: [u8; 3],
    /// Opacity of annotation strokes, 0-255.
    pub draw_alpha: u8,
    /// Width of annotation strokes in logical pixels (before DPI scaling).
    pub draw_width: f32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            dot_radius: 10.0,
            dot_color: [220, 30, 30],
            dot_alpha: 220,
            dot_stroke_width: 2.0,
            dot_stroke_alpha: 180,
            trail_duration_secs: 1.8,
            trail_max_radius: 12.0,
            trail_min_radius: 3.0,
            trail_alpha_max: 180,
            draw_color: [255, 200, 0],
            draw_alpha: 230,
            draw_width: 4.0,
        }
    }
}

/// Load `lasor.toml` from next to the executable, falling back to defaults.
pub fn load_config() -> Config {
    let path = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("lasor.toml")))
        .unwrap_or_else(|| std::path::PathBuf::from("lasor.toml"));

    match std::fs::read_to_string(&path) {
        Ok(contents) => toml::from_str(&contents).unwrap_or_else(|e| {
            eprintln!(
                "[lasor] failed to parse {}: {e}; using defaults",
                path.display()
            );
            Config::default()
        }),
        Err(_) => Config::default(),
    }
}
