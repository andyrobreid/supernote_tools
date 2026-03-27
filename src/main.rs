use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use regex::Regex;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Parser, Debug)]
#[command(
    version,
    about = "Sync Supernote files (.note/.txt/.pdf) to local files"
)]
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

    /// Path to supernote_pdf binary (legacy name retained for compatibility)
    #[arg(long, default_value = "supernote_pdf")]
    supernote_pdf_bin: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// List supported files discovered on the Supernote device
    Scan,
    /// Sync supported files to local output directory
    Sync {
        /// Output mode for .note files: auto, pdf, pdf-and-markdown, markdown-only
        #[arg(long, default_value = "auto")]
        mode: String,

        /// Pass --normalize-text-whitespace to supernote_pdf when markdown is involved
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
enum FileKind {
    Note,
    Text,
    Pdf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RemoteFileMeta {
    id: String,
    uri: String,
    date: String,
    size: u64,
    kind: FileKind,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct SyncState {
    files: HashMap<String, RemoteFileMeta>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Scan => {
            let files = fetch_all_supported_files(&cli.host, cli.port)?;
            println!("Found {} supported files", files.len());
            for f in files.iter().take(30) {
                println!("{:?}  {}  {}", f.kind, f.date, f.uri);
            }
            if files.len() > 30 {
                println!("... ({} more)", files.len() - 30);
            }
        }
        Commands::Sync {
            ref mode,
            normalize_text_whitespace,
        } => {
            sync_files(&cli, mode, normalize_text_whitespace)?;
        }
        Commands::Tui => {
            println!("supernote-tools TUI (MVP)\n");
            println!("This mode is scaffolded. Next steps:");
            println!("1) live device status panel");
            println!("2) selectable file list with changed/new badges");
            println!("3) one-key sync trigger");
            println!("4) live conversion logs");
        }
    }

    Ok(())
}

fn sync_files(cli: &Cli, mode: &str, normalize_text_whitespace: bool) -> Result<()> {
    validate_mode(mode)?;

    let files = fetch_all_supported_files(&cli.host, cli.port)?;
    fs::create_dir_all(&cli.out)?;

    let state_path = cli.out.join(".supernote-tools-state.json");
    let mut state = load_state(&state_path)?;

    let current_ids: HashSet<String> = files.iter().map(|f| f.id.clone()).collect();
    state.files.retain(|id, _| current_ids.contains(id));

    let mut changed = Vec::new();
    for file in &files {
        let needs = state
            .files
            .get(&file.id)
            .map(|old| old.date != file.date || old.size != file.size || old.kind != file.kind)
            .unwrap_or(true);
        if needs {
            changed.push(file.clone());
        }
    }

    println!(
        "{} files discovered; {} changed/new",
        files.len(),
        changed.len()
    );

    for file in changed {
        println!("Syncing {}", file.uri);

        let bytes = download_file(&cli.host, cli.port, &file.uri)?;
        let local_raw_path = cli.out.join(file.uri.trim_start_matches('/'));
        if let Some(parent) = local_raw_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&local_raw_path, &bytes)?;

        if file.kind == FileKind::Note {
            run_note_conversion(
                &cli.supernote_pdf_bin,
                mode,
                normalize_text_whitespace,
                &local_raw_path,
            )?;
        }

        state.files.insert(file.id.clone(), file);
    }

    save_state(&state_path, &state)?;
    println!("Sync complete");
    Ok(())
}

fn run_note_conversion(
    supernote_pdf_bin: &str,
    mode: &str,
    normalize_text_whitespace: bool,
    note_local_path: &Path,
) -> Result<()> {
    let mut output = note_local_path.to_path_buf();
    match mode {
        "pdf" | "pdf-and-markdown" | "auto" => {
            output.set_extension("pdf");
        }
        "markdown-only" => {
            output.set_extension("md");
        }
        _ => unreachable!(),
    }

    let mut cmd = Command::new(supernote_pdf_bin);
    cmd.arg("--input")
        .arg(note_local_path)
        .arg("--output")
        .arg(&output);

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
        "auto" => {
            cmd.arg("--auto-output");
            if normalize_text_whitespace {
                cmd.arg("--normalize-text-whitespace");
            }
        }
        _ => unreachable!(),
    }

    let status = cmd.status().context("failed to launch supernote_pdf")?;
    if !status.success() {
        bail!("supernote_pdf failed for {}", note_local_path.display());
    }

    Ok(())
}

fn validate_mode(mode: &str) -> Result<()> {
    match mode {
        "auto" | "pdf" | "pdf-and-markdown" | "markdown-only" => Ok(()),
        other => bail!("unsupported mode: {other}"),
    }
}

fn fetch_all_supported_files(host: &str, port: u16) -> Result<Vec<RemoteFileMeta>> {
    let client = Client::builder().build()?;
    let mut notes = Vec::new();

    for root in ["/Note", "/Document"] {
        let fetched = fetch_supported_files_under_root(&client, host, port, root)?;
        notes.extend(fetched);
    }

    notes.sort_by(|a, b| b.date.cmp(&a.date));
    notes.dedup_by(|a, b| a.uri == b.uri);
    Ok(notes)
}

fn fetch_supported_files_under_root(
    client: &Client,
    host: &str,
    port: u16,
    root: &str,
) -> Result<Vec<RemoteFileMeta>> {
    let mut stack = vec![root.to_string()];
    let mut out = Vec::new();

    while let Some(path) = stack.pop() {
        let body = match get_html(client, host, port, &path) {
            Ok(body) => body,
            Err(e) => {
                if path == root {
                    eprintln!("Skipping '{}' (unavailable): {}", root, e);
                    break;
                }
                return Err(e);
            }
        };

        let parsed = parse_embedded_json(&body)?;

        for f in parsed.file_list {
            if f.is_directory {
                stack.push(f.uri);
                continue;
            }

            let kind = match f.extension.as_deref() {
                Some("note") => Some(FileKind::Note),
                Some("txt") => Some(FileKind::Text),
                Some("pdf") => Some(FileKind::Pdf),
                _ => None,
            };

            if let Some(kind) = kind {
                out.push(RemoteFileMeta {
                    id: stable_id(&f.uri),
                    uri: f.uri,
                    date: f.date,
                    size: f.size,
                    kind,
                });
            }
        }
    }

    Ok(out)
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

fn download_file(host: &str, port: u16, uri: &str) -> Result<Vec<u8>> {
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
