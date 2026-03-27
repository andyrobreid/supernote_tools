# supernote-tools

Standalone sync toolkit (CLI today, TUI roadmap) for Supernote content.

It is fully standalone. If your workflow is Obsidian-centric, `supernote-companion` is an optional companion plugin rather than a requirement.

## What it syncs

- `.note` files (keeps raw `.note`, plus converted outputs)
- `.txt` files (synced as plain text)
- `.pdf` files (synced as-is)

Scans both `/Note` and `/Document` where available, preserving folder structure under your output root.

## Obsidian workflow (optional)

If you use Obsidian, you can pair this with the `supernote-companion` plugin:

- https://github.com/andyrobreid/supernote-companion

Use `supernote-tools` as your standalone sync/conversion backbone, and `supernote-companion` as an Obsidian-native workflow option.

## .note conversion modes

- `auto` (default): PDF always, Markdown only when recognized text exists
- `pdf`
- `pdf-and-markdown`
- `markdown-only`

## Build

```bash
cd /home/andyrobreid/dev/supernote-sync
# package/binary name is now supernote-tools (legacy supernote-sync binary still available)
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

- `./output/.supernote-tools-state.json`

Legacy state file `./output/.supernote-sync-state.json` is auto-read if present.

Used for incremental sync.
