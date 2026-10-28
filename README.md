# Twitch Lite

A lightweight, native Twitch client for Windows. Minimal resource usage, no Electron, no browser engine.

Built with Rust, egui/eframe (OpenGL), and libmpv for video playback.

![Twitch Lite](https://img.shields.io/badge/platform-Windows-blue)
![Rust](https://img.shields.io/badge/language-Rust-orange)

## Features

- Live stream playback via HLS (libmpv)
- Twitch chat with 7TV, BTTV, and FFZ emotes
- CJK text support (Japanese, Chinese, Korean)
- Animated emotes (GIF/WebP) with atlas packing
- Twemoji rendering
- Channel points, drops, subscriptions, notifications
- Ad detection and clean stream bypass
- Theater mode, always-on-top, chat overlay
- Keyboard shortcuts (K/Space, M, F, T, C)
- Custom dark theme matching Twitch's design
- Portable: single exe + libmpv-2.dll, no installer

## Requirements

- Windows 10/11
- [Rust toolchain](https://rustup.rs/) (for building)
- `libmpv-2.dll` — place it next to the built executable

### Getting libmpv

Download a recent mpv build (e.g. from [mpv.io](https://mpv.io/installation/) or [sourceforge](https://sourceforge.net/projects/mpv-player-windows/files/libmpv/)) and extract `libmpv-2.dll`. Place it in the same directory as `twitchlite.exe`.

Without libmpv the app still works — you get chat-only mode.

## Build

```bash
cargo build --release
```

The binary is at `target/release/twitchlite.exe`. Copy `libmpv-2.dll` next to it.

## Usage

Run `twitchlite.exe`. The UI is in Spanish (es-MX).

### Authentication

Two ways to log in:

1. **Session token** (recommended): Open twitch.tv in your browser, press F12 → Application → Cookies → copy the `auth-token` value. Paste it in the login dialog.

2. **Device Code**: If you have your own Twitch Client ID, you can use the Device Code OAuth flow from the login dialog.

Tokens are stored in the Windows Credential Manager, never in plain files.

### Without login

You can watch any public stream and read chat without logging in. Login is needed to send messages, see your follows, drops, subscriptions, and channel points.

## Configuration

Config is saved at `%APPDATA%\TwitchLite\config.json`. Image cache at `%LOCALAPPDATA%\TwitchLite\cache` (200 MB limit, auto-pruned).

## Architecture

| File | What it does |
|------|-------------|
| `main.rs` | App state, config, message parsing, window setup |
| `ui.rs` | All UI rendering (egui): player, chat, sidebar, menus |
| `twitch.rs` | Twitch API (GQL + Helix), IRC chat, auth, HLS |
| `mpv.rs` | libmpv FFI: loads DLL at runtime, OpenGL FBO rendering |
| `img.rs` | Image engine: SIEVE cache, worker threads, atlas packing |

## License

MIT
