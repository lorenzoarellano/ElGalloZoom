# ELGalloZoom (EGZ)

Smooth 120 FPS desktop screen magnifier and cursor spotlight overlay for **Windows 11** and **macOS**, designed to pair with mice such as the **Logitech MX Master**.

## Features

- **System Tray / Menu Bar Only**: Runs silently in the Windows System Tray or macOS Menu Bar without cluttering the Taskbar, Dock, or `Alt+Tab` / `Cmd+Tab`.
- **120 FPS Linear Interpolation (`lerp`)**: Smooth zoom transitions focused precisely on the cursor tip with Per-Monitor DPI Awareness (`DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2` on Windows, Retina HiDPI on macOS).
- **Click-Through Spotlight / Vignette Overlay**: Optional peripheral dimming around the cursor (`WS_EX_LAYERED | WS_EX_TRANSPARENT` on Windows, borderless click-through `NSWindow` on macOS).
- **Real-Time Settings & Persistence**: Built with `egui` / `eframe` and automatically persisted to `%APPDATA%/SmoothZoom/config.json` (Windows) or `~/Library/Application Support/SmoothZoom/config.json` (macOS).
- **Fail-Safe Cleanup**: Installs a panic hook and exit handler to guarantee magnification resets to `1.0x` on exit.

## Building

### Windows 11

```powershell
cargo build --release
# Binary output: target/release/el-gallo-zoom.exe
```

### macOS (`.app` Bundle & `.dmg` Installer)

```bash
chmod +x scripts/build_macos_dmg.sh
./scripts/build_macos_dmg.sh
# Installer output: target/release/ELGalloZoom.dmg
```

> **Note (macOS Accessibility Permission):** On first launch, grant Accessibility permission to **ELGalloZoom** under **System Settings → Privacy & Security → Accessibility** so `CGEventTapCreate` can intercept the zoom modifier + scroll wheel gesture.

