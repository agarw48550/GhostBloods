# GhostBloods Desktop — Installation & Usage

## Prerequisites

| Tool | Version | Install |
|------|---------|---------|
| Node.js | ≥ 22 | [nodejs.org](https://nodejs.org/) or `nvm install 22` |
| Rust | ≥ 1.75 | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| Xcode CLT | Latest | `xcode-select --install` |

## Quick Start

```bash
# 1. Clone and install
git clone https://github.com/agarw48550/GhostBloods.git
cd GhostBloods
npm install

# 2. Copy env (optional — works without API keys)
cp .env.example .env

# 3. Run desktop mode
cargo tauri dev

# OR run browser-only mode (no desktop shell)
npm run dev
```

## How It Works

### Two Modes

**Dashboard Mode** (foreground)
- Click "Open Dashboard" from the tray icon
- Starts the full Node.js engine (27 OSINT sources, SSE, 3D globe)
- Opens a native webview window at `localhost:3117`

**Notifier Mode** (background — default)
- When you close the dashboard window, the Node engine stops
- A lightweight loop runs every 45 minutes (configurable):
  - Queries GDELT (2 queries, 25 articles each)
  - Fetches ~20 curated RSS feeds
  - Scores articles by severity keywords + recency + watchlist
  - Sends native macOS notifications for FLASH/PRIORITY alerts
- **CPU/RAM impact**: Near-zero between sweeps. Each sweep takes ~10-20 seconds, then the process exits.

### Tray Menu

| Action | What it does |
|--------|-------------|
| 🌐 Open Dashboard | Starts engine + opens webview |
| 🔍 Force Background Check | Runs one lite sweep immediately |
| 🔇 Mute 1h/8h/24h | Suppresses notifications |
| ⚙️ Settings | Opens settings window |
| ✖ Quit | Stops engine + exits app |

## macOS Notification Permissions

On first run, macOS will ask to allow notifications from GhostBloods.

1. Click **Allow** when prompted
2. Or go to: **System Settings → Notifications → GhostBloods** → Enable

### Alert Tiers

| Tier | Icon | Criteria |
|------|------|----------|
| FLASH | 🔴 | Score ≥ 15 (nuclear, invasion, flash crash) |
| PRIORITY | 🟡 | Score ≥ threshold (conflict, sanctions, crisis) |

### Anti-spam

- Max 3 notifications per hour (configurable)
- Digest mode: >3 alerts → single summary notification
- Quiet hours: suppress non-FLASH during configured hours
- Content hashing: same event won't re-alert

## Settings

Open Settings from the tray menu to configure:

- **Sweep interval**: 30–120 minutes (default: 45)
- **Alert threshold**: 1–20 (default: 8, lower = more sensitive)
- **Notifications per hour**: 1–5 (default: 3)
- **Digest mode**: Combine multiple alerts (default: on)
- **Quiet hours**: Set start/end times (FLASH overrides)
- **Watchlist**: Keywords and regions that boost alert scores

Settings persist to `~/Library/Application Support/com.ghostbloods.app/notifier-state.json`.

## Verifying Background Mode

```bash
# After closing the dashboard window:

# 1. Check that the Node engine is NOT running
lsof -i :3117
# Should return nothing

# 2. Check memory usage
# Open Activity Monitor → search "GhostBloods"
# Should show < 50 MB RAM between sweeps

# 3. Check CPU usage
# Should show < 1% CPU between sweeps
```

## File Structure

```
GhostBloods/
├── src-tauri/              # Tauri v2 Rust shell
│   ├── src/
│   │   ├── lib.rs          # Entry + IPC commands
│   │   ├── engine.rs       # Node.js lifecycle
│   │   ├── tray.rs         # Menu bar
│   │   └── notifier.rs     # Background loop
│   └── tauri.conf.json     # App config
├── notifier/
│   ├── lite-sweep.mjs      # GDELT + RSS sweep
│   ├── scorer.mjs          # Alert scoring
│   └── rss-feeds.json      # 20 curated feeds
├── server.mjs              # Crucix engine (unchanged)
├── dashboard/public/
│   ├── jarvis.html         # Dashboard (rebranded)
│   └── settings.html       # Settings UI
└── docs/desktop.md         # This file
```

## Troubleshooting

**Dashboard won't open?**
```bash
# Check if port 3117 is in use
lsof -i :3117
# Kill stale process
kill $(lsof -ti:3117)
```

**No notifications?**
- Check macOS notification permissions
- Check if muted (tray menu)
- Lower the threshold in Settings
- Run "Force Background Check" to test

**Rust/Tauri build errors?**
```bash
# Update Rust
rustup update
# Rebuild
cargo tauri dev
```
