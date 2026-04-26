{ pkgs ? import <nixpkgs> {} }:

pkgs.mkShell {
  name = "lasor";

  nativeBuildInputs = with pkgs; [
    rustc
    cargo
    gcc
    pkg-config
  ];

  buildInputs = with pkgs; [
    # X11 backend (winit / eframe)
    xorg.libX11
    xorg.libXcursor
    xorg.libXrandr
    xorg.libXi
    xorg.libxcb

    # Wayland backend (optional; winit falls back to X11 via DISPLAY check)
    wayland
    libxkbcommon

    # OpenGL / wgpu
    libGL

    # Font rendering
    fontconfig
    freetype
  ];

  # Expose the system display-server libraries to the dynamic linker.
  # Without this the .so files are not on LD_LIBRARY_PATH even though they
  # are in buildInputs, because mkShell does not set rpath.
  LD_LIBRARY_PATH = with pkgs; pkgs.lib.makeLibraryPath [
    xorg.libX11
    xorg.libXcursor
    xorg.libXrandr
    xorg.libXi
    xorg.libxcb
    wayland
    libxkbcommon
    libGL
    fontconfig
    freetype
  ];
}
