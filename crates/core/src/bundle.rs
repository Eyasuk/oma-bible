//! Data built into the app, so a fresh install works offline.
//!
//! The King James Version and its study data ship inside the binary:
//! cross-references, the Matthew Henry commentary, the original Hebrew and
//! Greek words and the STEPBible lexicon. Everything else (other
//! translations and commentaries) still comes from the Bible API.
//!
//! The data lives in pack files under `crates/core/bundle/`, written by
//! `cargo run -p oma-bible-bundle`. A pack is an index followed by
//! separately compressed entries, so opening a chapter inflates only that
//! chapter. Entries are keyed by the same relative path the disk cache uses
//! (`open-cross-ref/GEN/1.json`), so a lookup is a drop-in for a download.
//!
//! Layout: `OMAPACK1`, a little-endian `u32` entry count, then per entry a
//! `u16` key length, the key, a `u32` offset and a `u32` length; then the
//! zlib-compressed entries, with offsets counted from the end of the index.

use std::{
    collections::HashMap,
    io::{self, Read, Write},
    sync::OnceLock,
};

use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};

use crate::bible::Translation;

const MAGIC: &[u8; 8] = b"OMAPACK1";

/// Packs built into the binary.
static PACKS: [&[u8]; 5] = [
    include_bytes!("../bundle/kjv.pack"),
    include_bytes!("../bundle/cross-references.pack"),
    include_bytes!("../bundle/commentary.pack"),
    include_bytes!("../bundle/interlinear.pack"),
    include_bytes!("../bundle/lexicon.pack"),
];

/// Abbreviation of the built-in translation, opened on a fresh install.
pub const DEFAULT_TRANSLATION: &str = "eng_kjv";
/// Language the download screen starts on when none was picked.
pub const DEFAULT_DOWNLOAD_LANGUAGE: &str = "eng";

/// Key of the built-in translation in `kjv.pack`.
pub const TRANSLATION_KEY: &str = "translation/eng_kjv.json";

/// A read-only view of one pack.
pub struct Pack {
    data: &'static [u8],
    entries: HashMap<&'static str, (usize, usize)>,
}

impl Pack {
    /// Reads a pack's index. Damaged or empty packs give an empty pack.
    pub fn parse(data: &'static [u8]) -> Self {
        let mut pack = Self {
            data,
            entries: HashMap::new(),
        };
        if let Some((entries, base)) = parse_index(data) {
            pack.entries = entries
                .into_iter()
                .map(|(key, offset, len)| (key, (base + offset, len)))
                .filter(|(_, (start, len))| start + len <= data.len())
                .collect();
        }
        pack
    }

    pub fn get(&self, key: &str) -> Option<Vec<u8>> {
        let &(start, len) = self.entries.get(key)?;
        let mut out = Vec::new();
        ZlibDecoder::new(&self.data[start..start + len])
            .read_to_end(&mut out)
            .ok()?;
        Some(out)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// An index entry: key, offset from the end of the index, compressed length.
type IndexEntry = (&'static str, usize, usize);

/// The entries and where the compressed data starts.
fn parse_index(data: &'static [u8]) -> Option<(Vec<IndexEntry>, usize)> {
    let mut at = 0usize;
    let mut take = |len: usize| -> Option<&'static [u8]> {
        let bytes = data.get(at..at + len)?;
        at += len;
        Some(bytes)
    };
    if take(8)? != MAGIC {
        return None;
    }
    let count = u32::from_le_bytes(take(4)?.try_into().ok()?) as usize;
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let key_len = u16::from_le_bytes(take(2)?.try_into().ok()?) as usize;
        let key = std::str::from_utf8(take(key_len)?).ok()?;
        let offset = u32::from_le_bytes(take(4)?.try_into().ok()?) as usize;
        let len = u32::from_le_bytes(take(4)?.try_into().ok()?) as usize;
        entries.push((key, offset, len));
    }
    Some((entries, at))
}

fn packs() -> &'static [Pack] {
    static PARSED: OnceLock<Vec<Pack>> = OnceLock::new();
    PARSED.get_or_init(|| PACKS.iter().map(|data| Pack::parse(data)).collect())
}

/// A built-in file by its cache path, inflated.
pub fn get(key: &str) -> Option<Vec<u8>> {
    packs().iter().find_map(|pack| pack.get(key))
}

/// The built-in King James Version, if this build carries it.
pub fn translation() -> Option<Translation> {
    let body = get(TRANSLATION_KEY)?;
    serde_json::from_slice(&body).ok()
}

/// The lexicon is packed 100 entries to a block (`H72` holds H7200–H7299),
/// since single entries are too small to compress well.
pub fn lexicon_block_key(strongs: &str) -> Option<String> {
    let prefix = strongs.get(..3)?;
    Some(format!("stepbible-lexicon/block/{prefix}.json"))
}

/// Writes a pack from `(key, uncompressed contents)` pairs.
pub fn write_pack<W: Write>(out: &mut W, entries: &[(String, Vec<u8>)]) -> io::Result<()> {
    let mut blobs = Vec::with_capacity(entries.len());
    for (key, contents) in entries {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
        encoder.write_all(contents)?;
        blobs.push((key, encoder.finish()?));
    }

    out.write_all(MAGIC)?;
    out.write_all(&(blobs.len() as u32).to_le_bytes())?;
    let mut offset = 0u32;
    for (key, blob) in &blobs {
        let key = key.as_bytes();
        let key_len = u16::try_from(key.len()).map_err(io::Error::other)?;
        out.write_all(&key_len.to_le_bytes())?;
        out.write_all(key)?;
        out.write_all(&offset.to_le_bytes())?;
        out.write_all(&(blob.len() as u32).to_le_bytes())?;
        offset = offset
            .checked_add(blob.len() as u32)
            .ok_or_else(|| io::Error::other("pack larger than 4 GB"))?;
    }
    for (_, blob) in &blobs {
        out.write_all(blob)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packs_round_trip() {
        let entries = vec![
            ("a/GEN/1.json".to_string(), b"{\"x\":1}".to_vec()),
            ("b".to_string(), vec![7; 5000]),
        ];
        let mut bytes = Vec::new();
        write_pack(&mut bytes, &entries).unwrap();
        let pack = Pack::parse(Box::leak(bytes.into_boxed_slice()));
        assert_eq!(pack.len(), 2);
        assert_eq!(pack.get("a/GEN/1.json").unwrap(), b"{\"x\":1}");
        assert_eq!(pack.get("b").unwrap(), vec![7; 5000]);
        assert!(pack.get("c").is_none());
    }

    #[test]
    fn damaged_packs_are_empty() {
        assert!(Pack::parse(b"nope").is_empty());
        assert!(Pack::parse(b"OMAPACK1\x05\x00\x00\x00").is_empty());
    }

    #[test]
    fn lexicon_blocks_group_a_hundred_entries() {
        assert_eq!(
            lexicon_block_key("H7225").as_deref(),
            Some("stepbible-lexicon/block/H72.json")
        );
    }
}

#[cfg(test)]
mod built_in {
    //! Run with the network blocked (e.g. `HTTPS_PROXY=http://127.0.0.1:9`)
    //! and an empty `OMA_BIBLE_CACHE_DIR` to prove the KJV study data needs
    //! no download.
    use crate::{original, study};

    #[test]
    fn the_kjv_and_its_study_data_are_built_in() {
        let kjv = super::translation().expect("built-in KJV");
        assert_eq!(kjv.abbreviation, super::DEFAULT_TRANSLATION);
        assert_eq!(kjv.books.len(), 66);

        let refs = study::cross_references("JHN", 3).unwrap();
        assert!(!refs.get(&16).unwrap().is_empty());

        let henry = study::commentary_chapter("matthew-henry", "GEN", 1)
            .unwrap()
            .expect("Matthew Henry on Genesis 1");
        assert!(henry.for_verse(1).is_some());
        assert!(!study::commentaries().unwrap().is_empty());

        let words = original::interlinear("GEN", 1).unwrap();
        assert_eq!(words[&1][2].strongs.as_deref(), Some("H0430"));
        let entry = original::lexicon("G3056").unwrap().expect("logos");
        assert_eq!(entry.senses[0].lemma, "λόγος");
    }
}
