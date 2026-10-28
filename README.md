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
- Ad detection, automatic freeze recovery, and return to live playback
- Compact purple refresh control below the video, with a pulse when recovery is needed
- Native desktop notifications with Twitch Lite branding
- Theater mode, always-on-top, chat overlay
- Keyboard shortcuts (K/Space, M, F, T, C, F5 to refresh)
- Custom dark theme matching Twitch's design
- Portable: single exe + libmpv-2.dll, no installer

## Quick Start

Download `twitchlite-portable.zip` from [Releases](https://github.com/romanovak/twitchlite/releases/latest), extract, and double-click `twitchlite.exe`. Everything is included.

If you run only the exe without the DLL, it will auto-download `libmpv-2.dll` on first launch. Without libmpv the app still works in chat-only mode.

## Requirements

- Windows 10/11

## Build from source

```bash
cargo build --release
```

The binary is at `target/release/twitchlite.exe`. The app auto-downloads `libmpv-2.dll` if missing, or you can place it next to the exe manually.

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

Config is saved at `%APPDATA%\TwitchLite\config.json`. Diagnostic logs are in `%APPDATA%\TwitchLite\twitchlite.log`. Image cache is at `%LOCALAPPDATA%\TwitchLite\cache` (200 MB limit, auto-pruned).

### Refresh and recovery

The purple refresh icon sits next to Share in the channel information below the video. It pulses when playback stops advancing, a retry is pending, or the player falls far behind live. It remains steady during normal playback. Hover for the tooltip, or press F5 to renew the playback session and return to live.

The player retries freezes automatically and reconnects when an ad break ends. Manual pause and rewind are respected. Network failures use increasing retry delays. Recovery depends on Twitch and the network; an uninterrupted stream or an ad-free alternative is not guaranteed.

### Validation

```powershell
cargo test --locked
```

The tests cover frozen video with advancing audio, startup, retry delays, intentional pause, the refresh attention state, and an ad playlist interrupted by a simulated network failure.

For a manual recovery check with the actual libmpv runtime, launch a single app instance with local diagnostic IPC enabled, then run the helper:

```powershell
$env:TWITCHLITE_MPV_IPC = '\\.\pipe\twitchlite-qa'
Start-Process .\dist\twitchlite.exe
.\scripts\test-recovery.ps1
Remove-Item Env:TWITCHLITE_MPV_IPC
```

The helper deliberately pauses mpv, verifies that the app reconnects without a manual refresh, and checks that playback advances afterward. Close and reopen the app normally after the check; IPC is disabled unless the environment variable is set. Verbose mpv logging is opt-in through `TWITCHLITE_MPV_LOG`; keep those logs private because playback URLs can contain session tokens.

## Architecture

| File | What it does |
|------|-------------|
| `main.rs` | App state, config, message parsing, window setup |
| `ui.rs` | All UI rendering (egui): player, chat, sidebar, menus |
| `twitch.rs` | Twitch API (GQL + Helix), IRC chat, auth, HLS |
| `mpv.rs` | libmpv FFI: loads DLL at runtime, OpenGL FBO rendering |
| `img.rs` | Image engine: SIEVE cache, worker threads, atlas packing |
| `playback.rs` | Playback health, retry timing, live latency, refresh attention state |
| `notifications.rs` | Native notifications and Windows application identity |

## License

MIT
