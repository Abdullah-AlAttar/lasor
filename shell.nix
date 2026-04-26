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
    libx11
    libxcursor
    libxrandr
    libxi
    libxcb

    # Wayland backend (optional; winit falls back to X11 via DISPLAY check)
    wayland
    libxkbcommon

    # OpenGL / Vulkan — required by wgpu to create a valid surface
    libGL
    mesa                 # software rasterizer + OpenGL ICD
    mesa.drivers         # DRI/DXVK drivers (Vulkan + GL ICDs)
    vulkan-loader        # Vulkan loader (libvulkan.so)
    vulkan-validation-layers

    # Font rendering
    fontconfig
    freetype
  ];

  # Expose the system display-server libraries to the dynamic linker.
  # Without this the .so files are not on LD_LIBRARY_PATH even though they
  # are in buildInputs, because mkShell does not set rpath.
  LD_LIBRARY_PATH = with pkgs; pkgs.lib.makeLibraryPath [
    libx11
    libxcursor
    libxrandr
    libxi
    libxcb
    wayland
    libxkbcommon
    libGL
    mesa
    mesa.drivers
    vulkan-loader
    fontconfig
    freetype
  ];

  # Tell wgpu to prefer the OpenGL backend.  On a machine without a real GPU
  # (VM, CI, headless nix-shell) Vulkan initialisation often fails with an
  # "Invalid surface" error; the GL backend via Mesa works reliably instead.
  WGPU_BACKEND = "gl";
}
