use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use regex::Regex;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Parser, Debug)]
#[command(version, about = "Sync Supernote .note files to local files without Obsidian")]
struct Cli {
    /// Supernote device host/IP
    #[arg(long, default_value = "192.168.86.26")]
    host: String,

    /// Supernote HTTP port (Browse & Access)
    #[arg(long, default_value_t = 8089)]
    port: u16,

    /// Local root output directory
    #[arg(long, default_value = "./output")]
    out: PathBuf,

    /// Path to supernote_pdf binary
    #[arg(long, default_value = "supernote_pdf")]
    supernote_pdf_bin: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// List notes discovered on the Supernote device
    Scan,
    /// Sync notes to local output directory
    Sync {
        /// Output mode: pdf, pdf-and-markdown, markdown-only
        #[arg(long, default_value = "markdown-only")]
        mode: String,

        /// Pass --normalize-text-whitespace to supernote_pdf
        #[arg(long, default_value_t = true)]
        normalize_text_whitespace: bool,
    },
    /// Simple TUI placeholder mode (interactive roadmap)
    Tui,
}

#[derive(Debug, Clone, Deserialize)]
struct DeviceResponse {
    #[serde(rename = "fileList")]
    file_list: Vec<DeviceFile>,
}

#[derive(Debug, Clone, Deserialize)]
struct DeviceFile {
    uri: String,
    extension: Option<String>,
    date: String,
    size: u64,
    #[serde(rename = "isDirectory")]
    is_directory: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NoteMeta {
    id: String,
    uri: String,
    date: String,
    size: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct SyncState {
    notes: HashMap<String, NoteMeta>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Scan => {
            let notes = fetch_all_notes(&cli.host, cli.port)?;
            println!("Found {} note files", notes.len());
            for n in notes.iter().take(20) {
                println!("{}  {}  {}", n.id, n.date, n.uri);
            }
            if notes.len() > 20 {
                println!("... ({} more)", notes.len() - 20);
            }
        }
        Commands::Sync {
            ref mode,
            normalize_text_whitespace,
        } => {
            sync_notes(&cli, mode, normalize_text_whitespace)?;
        }
        Commands::Tui => {
            println!("supernote-sync TUI (MVP)\n");
            println!("This mode is scaffolded. Next steps:");
            println!("1) live device status panel");
            println!("2) selectable note list with changed/new badges");
            println!("3) one-key sync trigger");
            println!("4) live conversion logs");
        }
    }

    Ok(())
}

fn sync_notes(cli: &Cli, mode: &str, normalize_text_whitespace: bool) -> Result<()> {
    let notes = fetch_all_notes(&cli.host, cli.port)?;
    fs::create_dir_all(&cli.out)?;

    let state_path = cli.out.join(".supernote-sync-state.json");
    let mut state = load_state(&state_path)?;

    let current_ids: HashSet<String> = notes.iter().map(|n| n.id.clone()).collect();
    state.notes.retain(|id, _| current_ids.contains(id));

    let mut changed = Vec::new();
    for note in &notes {
        let needs = state
            .notes
            .get(&note.id)
            .map(|old| old.date != note.date || old.size != note.size)
            .unwrap_or(true);
        if needs {
            changed.push(note.clone());
        }
    }

    println!("{} notes discovered; {} changed/new", notes.len(), changed.len());

    for note in changed {
        println!("Syncing {}", note.uri);
        let note_bytes = download_note(&cli.host, cli.port, &note.uri)?;

        let rel = note.uri.trim_start_matches("/Note/");
        let note_local_path = cli.out.join(rel);
        if let Some(parent) = note_local_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&note_local_path, &note_bytes)?;

        let mut output = note_local_path.clone();
        match mode {
            "pdf" => {
                output.set_extension("pdf");
            }
            "pdf-and-markdown" => {
                output.set_extension("pdf");
            }
            "markdown-only" => {
                output.set_extension("md");
            }
            other => bail!("unsupported mode: {other}"),
        }

        let mut cmd = Command::new(&cli.supernote_pdf_bin);
        cmd.arg("--input").arg(&note_local_path).arg("--output").arg(&output);

        match mode {
            "pdf" => {}
            "pdf-and-markdown" => {
                cmd.arg("--pdf-and-markdown");
                if normalize_text_whitespace {
                    cmd.arg("--normalize-text-whitespace");
                }
            }
            "markdown-only" => {
                cmd.arg("--markdown-only");
                if normalize_text_whitespace {
                    cmd.arg("--normalize-text-whitespace");
                }
            }
            _ => unreachable!(),
        }

        let status = cmd.status().context("failed to launch supernote_pdf")?;
        if !status.success() {
            bail!("supernote_pdf failed for {}", note.uri);
        }

        state.notes.insert(note.id.clone(), note);
    }

    save_state(&state_path, &state)?;
    println!("Sync complete");
    Ok(())
}

fn fetch_all_notes(host: &str, port: u16) -> Result<Vec<NoteMeta>> {
    let client = Client::builder().build()?;
    let mut stack = vec!["/Note".to_string()];
    let mut notes = Vec::new();

    while let Some(path) = stack.pop() {
        let body = get_html(&client, host, port, &path)?;
        let parsed = parse_embedded_json(&body)?;

        for f in parsed.file_list {
            if f.is_directory {
                stack.push(f.uri);
            } else if f.extension.as_deref() == Some("note") {
                notes.push(NoteMeta {
                    id: stable_id(&f.uri),
                    uri: f.uri,
                    date: f.date,
                    size: f.size,
                });
            }
        }
    }

    notes.sort_by(|a, b| b.date.cmp(&a.date));
    Ok(notes)
}

fn get_html(client: &Client, host: &str, port: u16, path: &str) -> Result<String> {
    let url = format!("http://{}:{}{}", host, port, path);
    let resp = client.get(url).send()?;
    let status = resp.status();
    let text = resp.text()?;
    if !status.is_success() {
        bail!("HTTP {} for {}", status, path);
    }
    Ok(text)
}

fn parse_embedded_json(html: &str) -> Result<DeviceResponse> {
    let re = Regex::new(r"const json = '(\{[^']+\})'")?;
    let m = re
        .captures(html)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str())
        .context("could not find embedded json in device response")?;
    let parsed: DeviceResponse = serde_json::from_str(m)?;
    Ok(parsed)
}

fn download_note(host: &str, port: u16, uri: &str) -> Result<Vec<u8>> {
    let encoded = uri
        .split('/')
        .map(|seg| {
            let mut s = urlencoding::encode(seg).into_owned();
            s = s.replace("%20", "+");
            s = s.replace("%2B", "+");
            s
        })
        .collect::<Vec<_>>()
        .join("/");

    let url = format!("http://{}:{}{}", host, port, encoded);
    let bytes = reqwest::blocking::get(url)?.bytes()?.to_vec();
    Ok(bytes)
}

fn stable_id(uri: &str) -> String {
    let mut hash: i32 = 0;
    for ch in uri.chars() {
        hash = ((hash << 5).wrapping_sub(hash)).wrapping_add(ch as i32);
    }
    format!("sn-{:x}", hash.unsigned_abs())
}

fn load_state(path: &Path) -> Result<SyncState> {
    if !path.exists() {
        return Ok(SyncState::default());
    }
    let data = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&data).unwrap_or_default())
}

fn save_state(path: &Path, state: &SyncState) -> Result<()> {
    let data = serde_json::to_string_pretty(state)?;
    fs::write(path, data)?;
    Ok(())
}

#[allow(dead_code)]
fn _parse_device_date(date: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_str(&(date.to_string() + ":00 +0000"), "%Y-%m-%d %H:%M:%S %z")
        .ok()
        .map(|d| d.with_timezone(&Utc))
}
