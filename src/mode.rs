/// The active operating mode of the overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// Overlay is idle – fully transparent, no interaction.
    #[default]
    Idle,
    /// Laser-pointer mode – animated dot + fading trail follows the cursor.
    Laser,
    /// Draw mode – the user can paint persistent annotation strokes.
    Draw,
}
