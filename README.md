# supernote-sync

Standalone CLI (with TUI roadmap) to sync Supernote content without Obsidian.

## What it syncs

- `.note` files (keeps raw `.note`, plus converted outputs)
- `.txt` files (synced as plain text)
- `.pdf` files (synced as-is)

Scans both `/Note` and `/Document` where available, preserving folder structure under your output root.

## .note conversion modes

- `auto` (default): PDF always, Markdown only when recognized text exists
- `pdf`
- `pdf-and-markdown`
- `markdown-only`

## Build

```bash
cd /home/andyrobreid/dev/supernote-sync
cargo build --release
```

## Usage

Scan supported files:

```bash
cargo run --release -- --host 192.168.86.26 --port 8089 --out ./output scan
```

Smart sync (default mode):

```bash
cargo run --release -- --host 192.168.86.26 --port 8089 --out ./output sync --mode auto --normalize-text-whitespace
```

Force markdown-only for `.note` files:

```bash
cargo run --release -- --host 192.168.86.26 --port 8089 --out ./output sync --mode markdown-only --normalize-text-whitespace
```

## Converter binary naming

The converter project now prefers `supernote_sync` as the CLI name, with `supernote_pdf` kept for compatibility.

This tool still accepts `--supernote-pdf-bin` for backward compatibility.

## State

Tracks file metadata in:

- `./output/.supernote-sync-state.json`

Used for incremental sync.
