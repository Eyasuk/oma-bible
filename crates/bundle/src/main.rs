//! Builds the data packs built into Oma Bible.
//!
//! ```text
//! cargo run --release -p oma-bible-bundle
//! ```
//!
//! Downloads the King James Version and, for every chapter, the
//! cross-references, the Matthew Henry commentary and the original Hebrew
//! and Greek words, plus every STEPBible lexicon entry, from the Free Use
//! Bible API. Raw downloads are kept in `target/bundle-cache` so an
//! interrupted run picks up where it stopped. The packs are written to
//! `crates/core/bundle/`; rebuild the app afterwards to embed them.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};

use oma_bible_core::{
    bundle::{self, TRANSLATION_KEY},
    remote::{API_BASE, DownloadControl, download_translation},
};
use serde_json::Value;

/// The commentary built in: the one the app opens by default.
const COMMENTARY: &str = "matthew-henry";
/// Parallel downloads; kept low to be polite to a free API.
const WORKERS: usize = 6;

/// Keys that only link to other API pages; dropping them saves space.
const LINK_KEYS: &[&str] = &[
    "dataset",
    "commentary",
    "translation",
    "book",
    "thisChapterLink",
    "thisChapterAudioLinks",
    "thisChapterReference",
    "nextChapterApiLink",
    "nextChapterAudioLinks",
    "nextChapterReference",
    "previousChapterApiLink",
    "previousChapterAudioLinks",
    "previousChapterReference",
    "numberOfVerses",
    "numberOfReferences",
    "thisLexiconEntryApiLink",
];

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = root.join("crates/core/bundle");
    let cache = root.join("target/bundle-cache");
    if let Err(error) = run(&out, &cache) {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run(out: &Path, cache: &Path) -> Result<(), String> {
    fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let client = reqwest::blocking::Client::builder()
        .user_agent("oma-bible-bundle")
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())?;

    println!("King James Version…");
    let kjv =
        download_translation("eng_kjv", &DownloadControl::new()).map_err(|e| e.to_string())?;
    let kjv_json = serde_json::to_vec(&kjv).map_err(|e| e.to_string())?;
    write(out, "kjv", vec![(TRANSLATION_KEY.to_string(), kjv_json)])?;

    let chapters: Vec<(String, u32)> = (0..kjv.books.len())
        .filter_map(|index| Some((kjv.book_id(index)?, index)))
        .flat_map(|(id, index)| {
            kjv.books[index]
                .chapters
                .iter()
                .map(move |chapter| (id.clone(), chapter.number))
                .collect::<Vec<_>>()
        })
        .collect();
    println!("{} chapters", chapters.len());

    let paths = |prefix: &str| -> Vec<String> {
        chapters
            .iter()
            .map(|(book, chapter)| format!("{prefix}/{book}/{chapter}.json"))
            .collect()
    };

    let cross = fetch_all(
        &client,
        cache,
        "cross-references",
        paths("open-cross-ref"),
        "d/",
    )?;
    write(out, "cross-references", cross)?;

    let mut commentary = fetch_all(
        &client,
        cache,
        "commentary",
        paths(&format!("c/{COMMENTARY}")),
        "",
    )?;
    if let Some(list) = fetch(&client, cache, "available_commentaries.json")? {
        commentary.push(("commentaries.json".to_string(), list));
    }
    write(out, "commentary", commentary)?;

    let interlinear = fetch_all(
        &client,
        cache,
        "original words",
        paths("bsb-interlinear"),
        "d/",
    )?;
    write(out, "interlinear", interlinear)?;

    let index = fetch(&client, cache, "d/stepbible-lexicon/lexicon.json")?
        .ok_or("the lexicon index is missing")?;
    let index: Value = serde_json::from_slice(&index).map_err(|e| e.to_string())?;
    let ids: Vec<String> = index["lexicon"]
        .as_array()
        .ok_or("the lexicon index has no entries")?
        .iter()
        .filter_map(|entry| entry["id"].as_str().map(str::to_string))
        .collect();
    let entries = fetch_all(
        &client,
        cache,
        "lexicon",
        ids.iter()
            .map(|id| format!("stepbible-lexicon/lexicon/{id}.json"))
            .collect(),
        "d/",
    )?;
    // Group entries a hundred to a block; single entries compress poorly.
    let mut blocks: BTreeMap<String, serde_json::Map<String, Value>> = BTreeMap::new();
    for (_, body) in entries {
        let value: Value = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
        let Some(entry) = value.get("entry") else {
            continue;
        };
        let Some(id) = entry["id"].as_str() else {
            continue;
        };
        if let Some(key) = bundle::lexicon_block_key(id) {
            blocks
                .entry(key)
                .or_default()
                .insert(id.to_string(), entry.clone());
        }
    }
    let blocks = blocks
        .into_iter()
        .map(|(key, map)| {
            (
                key,
                serde_json::to_vec(&Value::Object(map)).unwrap_or_default(),
            )
        })
        .collect();
    write(out, "lexicon", blocks)?;

    println!("Done. Rebuild the app to embed the new packs.");
    Ok(())
}

/// Fetches every API path (`{prefix}{path}` under `/api/`), keyed by `path`.
/// Missing pages (a commentary that skips a book) are left out.
fn fetch_all(
    client: &reqwest::blocking::Client,
    cache: &Path,
    label: &str,
    paths: Vec<String>,
    prefix: &str,
) -> Result<Vec<(String, Vec<u8>)>, String> {
    let total = paths.len();
    let queue = Mutex::new(paths.into_iter().enumerate().collect::<Vec<_>>());
    let done = AtomicUsize::new(0);
    let results = Mutex::new(Vec::with_capacity(total));
    let failure = Mutex::new(None::<String>);

    thread::scope(|scope| {
        for _ in 0..WORKERS {
            scope.spawn(|| {
                loop {
                    let Some((order, path)) = queue.lock().unwrap().pop() else {
                        break;
                    };
                    if failure.lock().unwrap().is_some() {
                        break;
                    }
                    match fetch(client, cache, &format!("{prefix}{path}")) {
                        Ok(Some(body)) => results.lock().unwrap().push((order, path, body)),
                        Ok(None) => {}
                        Err(error) => {
                            *failure.lock().unwrap() = Some(format!("{path}: {error}"));
                            break;
                        }
                    }
                    let count = done.fetch_add(1, Ordering::Relaxed) + 1;
                    if count.is_multiple_of(250) || count == total {
                        println!("{label}: {count}/{total}");
                    }
                }
            });
        }
    });

    if let Some(error) = failure.into_inner().unwrap() {
        return Err(error);
    }
    let mut results = results.into_inner().unwrap();
    results.sort_by_key(|(order, ..)| *order);
    Ok(results
        .into_iter()
        .map(|(_, path, body)| (path, body))
        .collect())
}

/// One API file, from the local download cache or the network, with the
/// link-only fields removed. `None` for a 404.
fn fetch(
    client: &reqwest::blocking::Client,
    cache: &Path,
    path: &str,
) -> Result<Option<Vec<u8>>, String> {
    let file = cache.join(path);
    let missing = file.with_extension("missing");
    if missing.exists() {
        return Ok(None);
    }
    let raw = if let Ok(raw) = fs::read(&file) {
        raw
    } else {
        let url = format!("{API_BASE}/api/{path}");
        let mut attempt = 0;
        let raw = loop {
            attempt += 1;
            let result = client.get(&url).send().and_then(|response| {
                let status = response.status();
                response.bytes().map(|bytes| (status, bytes))
            });
            match result {
                Ok((status, _)) if status == reqwest::StatusCode::NOT_FOUND => {
                    if let Some(parent) = missing.parent() {
                        let _ = fs::create_dir_all(parent);
                    }
                    let _ = fs::write(&missing, b"");
                    return Ok(None);
                }
                Ok((status, bytes)) if status.is_success() => break bytes.to_vec(),
                Ok((status, _)) if attempt >= 4 => return Err(format!("HTTP {status}")),
                Err(error) if attempt >= 4 => return Err(error.to_string()),
                _ => thread::sleep(Duration::from_secs(2 * attempt)),
            }
        };
        if let Some(parent) = file.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::write(&file, &raw).map_err(|e| e.to_string())?;
        raw
    };

    let mut value: Value = serde_json::from_slice(&raw).map_err(|e| format!("{path}: {e}"))?;
    if let Some(object) = value.as_object_mut() {
        for key in LINK_KEYS {
            object.remove(*key);
        }
    }
    serde_json::to_vec(&value)
        .map(Some)
        .map_err(|e| e.to_string())
}

fn write(out: &Path, name: &str, mut entries: Vec<(String, Vec<u8>)>) -> Result<(), String> {
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let mut bytes = Vec::new();
    bundle::write_pack(&mut bytes, &entries).map_err(|e| e.to_string())?;
    let path = out.join(format!("{name}.pack"));
    fs::write(&path, &bytes).map_err(|e| e.to_string())?;
    println!(
        "wrote {} ({} entries, {:.1} MB)",
        path.display(),
        entries.len(),
        bytes.len() as f64 / 1_048_576.0
    );
    Ok(())
}
