# supernote-sync

Standalone CLI (with TUI roadmap) to sync Supernote `.note` files without Obsidian.

## Features (MVP)

- Connects to Supernote Browse & Access HTTP API (`:8089`)
- Recursively scans `/Note`
- Detects changed/new notes via local state file
- Downloads `.note` files
- Invokes `supernote_pdf` for conversion
  - `pdf`
  - `pdf-and-markdown`
  - `markdown-only`

## Build

```bash
cd /home/andyrobreid/dev/supernote-sync
cargo build --release
```

## Usage

Scan device notes:

```bash
cargo run --release -- --host 192.168.86.26 --port 8089 --out ./output scan
```

Sync in markdown-only mode:

```bash
cargo run --release -- --host 192.168.86.26 --port 8089 --out ./output sync --mode markdown-only --normalize-text-whitespace
```

Sync in pdf-and-markdown mode:

```bash
cargo run --release -- --host 192.168.86.26 --port 8089 --out ./output sync --mode pdf-and-markdown --normalize-text-whitespace
```

TUI placeholder:

```bash
cargo run --release -- --host 192.168.86.26 tui
```

## State

Tracks note metadata in:

- `./output/.supernote-sync-state.json`

This powers incremental sync.

## Next roadmap

- Real ratatui dashboard
- Selection filters
- Per-note diff summary
- Cron/watch mode
- Parallel conversion queue
