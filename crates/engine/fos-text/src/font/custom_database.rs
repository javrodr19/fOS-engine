//! Font database: system font discovery and matching.
//!
//! Scanning reads only each font file's table directory and its `name`,
//! `OS/2` and `head` tables (a few KB per file) instead of loading every
//! font on the system into memory. Face data is loaded lazily the first
//! time a font is actually used, and is then shared by every user of the
//! database (see [`CustomFontDatabase::shared`]).

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use super::{FontStyle, FontWeight, FontQuery, resolve_generic_family};
use crate::{Result, TextError};

/// Unique font identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FontId(pub u32);

/// The bytes of a font face; dereferences to `[u8]`, and cloning is free.
///
/// Font files are memory-mapped rather than read: only the pages actually
/// used (a few tables and the outlines of the glyphs drawn) become
/// resident, and they are clean file-backed pages that the kernel can drop
/// under memory pressure and share between processes. Decoded web fonts
/// and fonts loaded from memory live on the heap.
#[derive(Clone)]
pub struct FaceData(Arc<FaceBytes>);

enum FaceBytes {
    Mapped(memmap2::Mmap),
    Owned(Vec<u8>),
}

impl FaceData {
    /// Face data held in memory
    pub fn owned(data: Vec<u8>) -> Self {
        Self(Arc::new(FaceBytes::Owned(data)))
    }

    /// Map a font file into memory
    fn map(file: &File) -> Option<Self> {
        // SAFETY: the mapping is read-only. As in other browsers, fonts are
        // assumed not to be truncated while in use; a font file replaced by
        // a package update keeps the old inode alive for this mapping.
        let map = unsafe { memmap2::Mmap::map(file) }.ok()?;
        Some(Self(Arc::new(FaceBytes::Mapped(map))))
    }

    /// Whether the data is a file mapping (as opposed to heap memory)
    pub fn is_mapped(&self) -> bool {
        matches!(*self.0, FaceBytes::Mapped(_))
    }
}

impl std::ops::Deref for FaceData {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        match &*self.0 {
            FaceBytes::Mapped(map) => map,
            FaceBytes::Owned(data) => data,
        }
    }
}

impl AsRef<[u8]> for FaceData {
    fn as_ref(&self) -> &[u8] {
        self
    }
}

impl std::fmt::Debug for FaceData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FaceData").field("len", &self.len()).field("mapped", &self.is_mapped()).finish()
    }
}

/// Font entry in database
pub struct FontEntry {
    /// Font ID
    pub id: FontId,
    /// Font family name (typographic family when available)
    pub family: String,
    /// Full font name
    pub full_name: String,
    /// PostScript name
    pub postscript_name: Option<String>,
    /// Font style
    pub style: FontStyle,
    /// Font weight
    pub weight: FontWeight,
    /// Font data source
    pub source: FontSource,
    /// Index in font file (for TTC)
    pub index: u32,
    /// Whether the face has TrueType (`glyf`) outlines. The rasterizer only
    /// supports these, so matching prefers them over CFF-only faces, which
    /// would otherwise render as invisible text.
    pub has_glyf: bool,
    /// Face data for file-backed fonts, loaded on first use
    data: OnceLock<Option<FaceData>>,
}

impl std::fmt::Debug for FontEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FontEntry")
            .field("id", &self.id)
            .field("family", &self.family)
            .field("full_name", &self.full_name)
            .field("style", &self.style)
            .field("weight", &self.weight)
            .field("source", &self.source)
            .field("index", &self.index)
            .field("has_glyf", &self.has_glyf)
            .field("loaded", &self.data.get().is_some())
            .finish()
    }
}

/// Font data source
#[derive(Debug, Clone)]
pub enum FontSource {
    /// File path
    File(PathBuf),
    /// Embedded data (already decoded from WOFF/WOFF2)
    Memory(FaceData),
}

/// Font database with custom implementation
pub struct CustomFontDatabase {
    /// All font entries, indexed by `FontId`
    fonts: Vec<FontEntry>,
    /// Lowercased family name → faces in that family
    by_family: HashMap<String, Vec<FontId>>,
}

impl CustomFontDatabase {
    /// Create a new empty database
    pub fn new() -> Self {
        Self {
            fonts: Vec::new(),
            by_family: HashMap::new(),
        }
    }

    /// Create with system fonts loaded
    pub fn with_system_fonts() -> Self {
        let mut db = Self::new();
        db.load_system_fonts();
        db
    }

    /// Process-wide database of system fonts, scanned once on first use.
    ///
    /// Sharing one database means the font directories are scanned once per
    /// process and each face's data is loaded at most once, no matter how
    /// many renderers (or threads) use it.
    pub fn shared() -> Arc<CustomFontDatabase> {
        static SHARED: OnceLock<Arc<CustomFontDatabase>> = OnceLock::new();
        SHARED.get_or_init(|| Arc::new(Self::with_system_fonts())).clone()
    }

    /// Load system fonts
    pub fn load_system_fonts(&mut self) {
        let mut dirs: Vec<PathBuf> = Vec::new();

        #[cfg(target_os = "linux")]
        {
            let home = std::env::var_os("HOME").map(PathBuf::from);
            let data_home = std::env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .or_else(|| home.as_ref().map(|h| h.join(".local/share")));
            if let Some(data_home) = data_home {
                dirs.push(data_home.join("fonts"));
            }
            if let Some(home) = &home {
                dirs.push(home.join(".fonts"));
            }

            let data_dirs = std::env::var("XDG_DATA_DIRS")
                .unwrap_or_else(|_| "/usr/local/share:/usr/share".to_string());
            for dir in data_dirs.split(':').filter(|d| !d.is_empty()) {
                dirs.push(Path::new(dir).join("fonts"));
            }
            dirs.push(PathBuf::from("/usr/share/fonts"));
            dirs.push(PathBuf::from("/usr/local/share/fonts"));
        }

        #[cfg(target_os = "macos")]
        {
            dirs.push(PathBuf::from("/System/Library/Fonts"));
            dirs.push(PathBuf::from("/Library/Fonts"));
            if let Some(home) = std::env::var_os("HOME") {
                dirs.push(PathBuf::from(home).join("Library/Fonts"));
            }
        }

        #[cfg(target_os = "windows")]
        {
            if let Some(windir) = std::env::var_os("WINDIR") {
                dirs.push(PathBuf::from(windir).join("Fonts"));
            }
            if let Some(local) = std::env::var_os("LOCALAPPDATA") {
                dirs.push(PathBuf::from(local).join("Microsoft\\Windows\\Fonts"));
            }
        }

        let mut visited = HashSet::new();
        for dir in dirs {
            self.scan_directory(&dir, 0, &mut visited);
        }
    }

    /// Scan a directory for fonts (recursively, guarding against symlink loops)
    fn scan_directory(&mut self, dir: &Path, depth: usize, visited: &mut HashSet<PathBuf>) {
        const MAX_DEPTH: usize = 8;
        if depth > MAX_DEPTH {
            return;
        }
        let Ok(canonical) = dir.canonicalize() else { return };
        if !visited.insert(canonical) {
            return;
        }

        let Ok(entries) = std::fs::read_dir(dir) else { return };
        let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        // Deterministic order, so font IDs and fallbacks are stable across runs
        paths.sort();

        for path in paths {
            if path.is_dir() {
                self.scan_directory(&path, depth + 1, visited);
            } else if let Some(ext) = path.extension() {
                let ext = ext.to_string_lossy().to_ascii_lowercase();
                if matches!(ext.as_str(), "ttf" | "otf" | "ttc" | "otc" | "woff" | "woff2") {
                    let _ = self.load_font_file(&path);
                }
            }
        }
    }

    /// Register the faces of a font file. Only metadata is read here; the
    /// face data itself is loaded on first use.
    pub fn load_font_file(&mut self, path: &Path) -> Result<Vec<FontId>> {
        let mut file = File::open(path)
            .map_err(|e| TextError::FontParsing(e.to_string()))?;
        let mut magic = [0u8; 4];
        file.read_exact(&mut magic)
            .map_err(|e| TextError::FontParsing(e.to_string()))?;

        let faces = if &magic == b"wOFF" || &magic == b"wOF2" {
            // Compressed web font: the tables are only readable after decoding
            let data = std::fs::read(path)
                .map_err(|e| TextError::FontParsing(e.to_string()))?;
            let decoded = decode_web_font(&data)?;
            scan_faces(&mut decoded.as_slice())
        } else {
            scan_faces(&mut file)
        };

        if faces.is_empty() {
            return Err(TextError::FontParsing(format!("No usable faces in {}", path.display())));
        }
        Ok(self.add_faces(faces, FontSource::File(path.to_path_buf())))
    }

    /// Load font from memory (TrueType, OpenType, WOFF or WOFF2)
    pub fn load_font_data(&mut self, data: Vec<u8>) -> Result<Vec<FontId>> {
        let data = if super::woff2::is_woff2(&data) || super::woff::is_woff(&data) {
            decode_web_font(&data)?
        } else {
            data
        };

        let faces = scan_faces(&mut data.as_slice());
        if faces.is_empty() {
            return Err(TextError::FontParsing("No usable faces in font data".into()));
        }
        Ok(self.add_faces(faces, FontSource::Memory(FaceData::owned(data))))
    }

    fn add_faces(&mut self, faces: Vec<(u32, FaceMetadata)>, source: FontSource) -> Vec<FontId> {
        let mut ids = Vec::with_capacity(faces.len());

        for (index, meta) in faces {
            let id = FontId(self.fonts.len() as u32);

            for family in &meta.families {
                let members = self.by_family.entry(family.to_lowercase()).or_default();
                if !members.contains(&id) {
                    members.push(id);
                }
            }

            let mut families = meta.families.into_iter();
            self.fonts.push(FontEntry {
                id,
                family: families.next().unwrap_or_default(),
                full_name: meta.full_name,
                postscript_name: meta.postscript_name,
                style: meta.style,
                weight: meta.weight,
                source: source.clone(),
                index,
                has_glyf: meta.has_glyf,
                data: OnceLock::new(),
            });
            ids.push(id);
        }

        ids
    }

    /// Query for a matching font.
    ///
    /// Families are tried in order; generic families (`sans-serif`,
    /// `monospace`, ...) expand to common concrete families. If nothing
    /// matches, the most suitable general-purpose text face is returned.
    pub fn query(&self, query: &FontQuery) -> Option<FontId> {
        for family in &query.families {
            let generic = resolve_generic_family(family);
            if generic.is_empty() {
                if let Some(id) = self.best_in_family(family, query) {
                    return Some(id);
                }
            } else {
                for concrete in generic {
                    if let Some(id) = self.best_in_family(concrete, query) {
                        return Some(id);
                    }
                }
            }
        }

        self.fonts.iter()
            .min_by_key(|f| fallback_score(f, query))
            .map(|f| f.id)
    }

    /// Best face within one family for the requested weight and style
    fn best_in_family(&self, family: &str, query: &FontQuery) -> Option<FontId> {
        self.by_family.get(&family.to_lowercase())?
            .iter()
            .filter_map(|id| self.font(*id))
            .min_by_key(|f| face_score(f, query))
            .map(|f| f.id)
    }

    /// Get font by ID
    pub fn font(&self, id: FontId) -> Option<&FontEntry> {
        self.fonts.get(id.0 as usize)
    }

    /// Face data for a font, loaded on first use and cached for the lifetime
    /// of the database. Cloning the returned `Arc` is free.
    pub fn face_data(&self, id: FontId) -> Option<FaceData> {
        let font = self.font(id)?;
        match &font.source {
            FontSource::Memory(data) => Some(data.clone()),
            FontSource::File(path) => font.data.get_or_init(|| load_file_data(path)).clone(),
        }
    }

    /// Get font data by ID
    pub fn with_face_data<R>(&self, id: FontId, f: impl FnOnce(&[u8], u32) -> R) -> Option<R> {
        let index = self.font(id)?.index;
        let data = self.face_data(id)?;
        Some(f(&data, index))
    }

    /// List all font families (lowercased)
    pub fn families(&self) -> impl Iterator<Item = &str> {
        self.by_family.keys().map(String::as_str)
    }

    /// Number of fonts
    pub fn len(&self) -> usize {
        self.fonts.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.fonts.is_empty()
    }
}

impl Default for CustomFontDatabase {
    fn default() -> Self {
        Self::new()
    }
}

/// How well a face matches the requested weight/style (lower is better)
fn face_score(font: &FontEntry, query: &FontQuery) -> u32 {
    let mut score = (font.weight.0 as i32 - query.weight.0 as i32).unsigned_abs();
    if font.style != query.style {
        score += 1_000;
    }
    if !font.has_glyf {
        score += 100_000;
    }
    score
}

/// Suitability of a face as a general-purpose fallback (lower is better)
fn fallback_score(font: &FontEntry, query: &FontQuery) -> u32 {
    const UNSUITABLE: &[&str] = &[
        "emoji", "symbol", "dingbat", "math", "music", "braille", "icon",
        "awesome", "wingding", "webding", "ornament", "cursor",
    ];

    let family = font.family.to_lowercase();
    let mut score = face_score(font, query);
    if UNSUITABLE.iter().any(|word| family.contains(word)) {
        score += 50_000;
    }
    if !family.contains("sans") {
        score += 2_000;
    }
    if family.contains("mono") {
        score += 3_000;
    }
    // Prefer base families ("Noto Sans") over script-specific ones ("Noto Sans Adlam")
    score + family.len() as u32
}

/// Map a font file, or read and decode it if it is a web font
fn load_file_data(path: &Path) -> Option<FaceData> {
    let mut file = File::open(path).ok()?;
    let mut magic = [0u8; 4];
    file.read_exact(&mut magic).ok()?;
    if &magic == b"wOFF" || &magic == b"wOF2" {
        let data = std::fs::read(path).ok()?;
        return decode_web_font(&data).ok().map(FaceData::owned);
    }
    FaceData::map(&file).or_else(|| std::fs::read(path).ok().map(FaceData::owned))
}

fn decode_web_font(data: &[u8]) -> Result<Vec<u8>> {
    if super::woff2::is_woff2(data) {
        super::woff2::decode_woff2(data)
            .ok_or_else(|| TextError::FontParsing("Failed to decode WOFF2".into()))
    } else {
        super::woff::decode_woff(data)
            .ok_or_else(|| TextError::FontParsing("Failed to decode WOFF1".into()))
    }
}

/// Random-access reads from a file or an in-memory buffer
trait ReadAt {
    fn read_at(&mut self, offset: u64, len: usize) -> Option<Vec<u8>>;
}

impl ReadAt for File {
    fn read_at(&mut self, offset: u64, len: usize) -> Option<Vec<u8>> {
        self.seek(SeekFrom::Start(offset)).ok()?;
        let mut buf = vec![0u8; len];
        self.read_exact(&mut buf).ok()?;
        Some(buf)
    }
}

impl ReadAt for &[u8] {
    fn read_at(&mut self, offset: u64, len: usize) -> Option<Vec<u8>> {
        let start = usize::try_from(offset).ok()?;
        self.get(start..start.checked_add(len)?).map(<[u8]>::to_vec)
    }
}

fn be_u16(data: &[u8], offset: usize) -> Option<u16> {
    data.get(offset..offset + 2).map(|b| u16::from_be_bytes([b[0], b[1]]))
}

fn be_u32(data: &[u8], offset: usize) -> Option<u32> {
    data.get(offset..offset + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

/// Metadata of one face, read from its `name`, `OS/2` and `head` tables
#[derive(Debug, Clone, PartialEq)]
struct FaceMetadata {
    /// Family names, primary first (typographic family, then legacy family)
    families: Vec<String>,
    full_name: String,
    postscript_name: Option<String>,
    style: FontStyle,
    weight: FontWeight,
    has_glyf: bool,
}

/// Enumerate the faces of a font file or collection
fn scan_faces<R: ReadAt>(src: &mut R) -> Vec<(u32, FaceMetadata)> {
    const MAX_FACES: u32 = 256;

    let Some(header) = src.read_at(0, 12) else { return Vec::new() };

    if &header[0..4] == b"ttcf" {
        let count = be_u32(&header, 8).unwrap_or(0).min(MAX_FACES);
        let Some(offsets) = src.read_at(12, count as usize * 4) else { return Vec::new() };
        (0..count)
            .filter_map(|i| {
                let offset = be_u32(&offsets, i as usize * 4)?;
                read_face_metadata(src, offset as u64).map(|meta| (i, meta))
            })
            .collect()
    } else {
        read_face_metadata(src, 0).map(|meta| vec![(0, meta)]).unwrap_or_default()
    }
}

/// Read the metadata of the face whose offset table starts at `face_offset`
fn read_face_metadata<R: ReadAt>(src: &mut R, face_offset: u64) -> Option<FaceMetadata> {
    const MAX_NAME_TABLE: usize = 1 << 20;

    let header = src.read_at(face_offset, 12)?;
    let version = be_u32(&header, 0)?;
    // TrueType, OpenType/CFF ('OTTO'), and legacy Apple TrueType ('true')
    if !matches!(version, 0x0001_0000 | 0x4F54_544F | 0x7472_7565) {
        return None;
    }

    let num_tables = be_u16(&header, 4)? as usize;
    if num_tables == 0 || num_tables > 512 {
        return None;
    }
    let records = src.read_at(face_offset + 12, num_tables * 16)?;

    let mut name = None;
    let mut os2 = None;
    let mut head = None;
    let mut has_glyf = false;
    for record in records.chunks_exact(16) {
        // Table offsets are relative to the start of the file, even in collections
        let location = (be_u32(record, 8)? as u64, be_u32(record, 12)? as usize);
        match &record[0..4] {
            b"name" => name = Some(location),
            b"OS/2" => os2 = Some(location),
            b"head" => head = Some(location),
            b"glyf" => has_glyf = true,
            _ => {}
        }
    }

    let (name_offset, name_len) = name?;
    let names = parse_name_table(&src.read_at(name_offset, name_len.min(MAX_NAME_TABLE))?);

    let mut weight = None;
    let mut italic = false;
    let mut oblique = false;
    if let Some((offset, len)) = os2 {
        if let Some(os2) = src.read_at(offset, len.min(64)) {
            weight = be_u16(&os2, 4);
            if let Some(fs_selection) = be_u16(&os2, 62) {
                italic = fs_selection & 0x0001 != 0;
                oblique = fs_selection & 0x0200 != 0;
            }
        }
    }

    // Fall back to head.macStyle (bit 0: bold, bit 1: italic)
    if let Some((offset, len)) = head {
        if len >= 46 {
            if let Some(mac_style) = src.read_at(offset + 44, 2).and_then(|d| be_u16(&d, 0)) {
                if weight.is_none() && mac_style & 0x1 != 0 {
                    weight = Some(700);
                }
                italic |= mac_style & 0x2 != 0;
            }
        }
    }

    let weight = match weight.unwrap_or(400) {
        // Some old fonts use a 1-9 scale
        w @ 1..=9 => w * 100,
        w => w.clamp(1, 1000),
    };
    let style = if italic {
        FontStyle::Italic
    } else if oblique {
        FontStyle::Oblique
    } else {
        FontStyle::Normal
    };

    let mut families = Vec::with_capacity(2);
    for family in [names.typographic_family, names.family].into_iter().flatten() {
        let family = family.trim().to_string();
        if !family.is_empty() && !families.contains(&family) {
            families.push(family);
        }
    }
    if families.is_empty() {
        return None;
    }

    Some(FaceMetadata {
        full_name: names.full_name.unwrap_or_else(|| families[0].clone()),
        families,
        postscript_name: names.postscript_name,
        style,
        weight: FontWeight(weight),
        has_glyf,
    })
}

/// Names extracted from the `name` table
#[derive(Debug, Default)]
struct FontNames {
    family: Option<String>,
    full_name: Option<String>,
    postscript_name: Option<String>,
    typographic_family: Option<String>,
}

/// Parse the `name` table, preferring English Windows/Unicode records
fn parse_name_table(data: &[u8]) -> FontNames {
    let mut names = FontNames::default();
    let mut best_rank = [u8::MAX; 4];

    let (Some(count), Some(string_offset)) = (be_u16(data, 2), be_u16(data, 4)) else {
        return names;
    };

    for i in 0..count as usize {
        let rec = 6 + i * 12;
        let (Some(platform), Some(encoding), Some(language), Some(name_id), Some(length), Some(offset)) = (
            be_u16(data, rec),
            be_u16(data, rec + 2),
            be_u16(data, rec + 4),
            be_u16(data, rec + 6),
            be_u16(data, rec + 8),
            be_u16(data, rec + 10),
        ) else {
            break;
        };

        let slot = match name_id {
            1 => 0,
            4 => 1,
            6 => 2,
            16 => 3,
            _ => continue,
        };

        // Lower rank is better
        let rank = match (platform, language) {
            (3, 0x0409) => 0,
            (3, _) => 1,
            (0, _) => 2,
            (1, 0) => 3,
            _ => continue,
        };
        if rank >= best_rank[slot] {
            continue;
        }

        let start = string_offset as usize + offset as usize;
        let Some(bytes) = data.get(start..start + length as usize) else { continue };
        let Some(value) = decode_name(platform, encoding, bytes) else { continue };
        if value.trim().is_empty() {
            continue;
        }

        best_rank[slot] = rank;
        let target = match slot {
            0 => &mut names.family,
            1 => &mut names.full_name,
            2 => &mut names.postscript_name,
            _ => &mut names.typographic_family,
        };
        *target = Some(value);
    }

    names
}

fn decode_name(platform: u16, encoding: u16, bytes: &[u8]) -> Option<String> {
    match platform {
        // Unicode and Windows platforms store UTF-16BE
        0 | 3 => {
            let units: Vec<u16> = bytes.chunks_exact(2)
                .map(|c| u16::from_be_bytes([c[0], c[1]]))
                .collect();
            Some(String::from_utf16_lossy(&units))
        }
        // Macintosh Roman; family names are ASCII in practice
        1 if encoding == 0 => Some(bytes.iter().map(|&b| b as char).collect()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a `name` table with the given (platform, encoding, language, name_id, string) records
    fn name_table(records: &[(u16, u16, u16, u16, &str)]) -> Vec<u8> {
        let mut strings = Vec::new();
        let mut recs = Vec::new();
        for &(platform, encoding, language, name_id, value) in records {
            let bytes: Vec<u8> = if platform == 1 {
                value.bytes().collect()
            } else {
                value.encode_utf16().flat_map(|u| u.to_be_bytes()).collect()
            };
            for v in [platform, encoding, language, name_id, bytes.len() as u16, strings.len() as u16] {
                recs.extend_from_slice(&v.to_be_bytes());
            }
            strings.extend_from_slice(&bytes);
        }
        let mut table = Vec::new();
        table.extend_from_slice(&0u16.to_be_bytes());
        table.extend_from_slice(&(records.len() as u16).to_be_bytes());
        table.extend_from_slice(&(6 + recs.len() as u16).to_be_bytes());
        table.extend_from_slice(&recs);
        table.extend_from_slice(&strings);
        table
    }

    /// Build a minimal single-face font with name, OS/2 and optionally glyf tables
    fn font_bytes(family: &str, weight: u16, italic: bool, glyf: bool) -> Vec<u8> {
        let name = name_table(&[
            (1, 0, 0, 1, "Mac Family"),
            (3, 1, 0x0409, 1, family),
            (3, 1, 0x0409, 4, &format!("{} Full", family)),
        ]);
        let mut os2 = vec![0u8; 78];
        os2[4..6].copy_from_slice(&weight.to_be_bytes());
        if italic {
            os2[62..64].copy_from_slice(&1u16.to_be_bytes());
        }

        let mut tables: Vec<(&[u8; 4], Vec<u8>)> = vec![(b"name", name), (b"OS/2", os2)];
        if glyf {
            tables.push((b"glyf", vec![0; 4]));
        }

        let mut out = Vec::new();
        out.extend_from_slice(&0x0001_0000u32.to_be_bytes());
        out.extend_from_slice(&(tables.len() as u16).to_be_bytes());
        out.extend_from_slice(&[0; 6]);
        let mut offset = 12 + tables.len() * 16;
        let mut body = Vec::new();
        for (tag, data) in &tables {
            out.extend_from_slice(*tag);
            out.extend_from_slice(&0u32.to_be_bytes());
            out.extend_from_slice(&(offset as u32).to_be_bytes());
            out.extend_from_slice(&(data.len() as u32).to_be_bytes());
            body.extend_from_slice(data);
            offset += data.len();
        }
        out.extend_from_slice(&body);
        out
    }

    #[test]
    fn test_empty_database() {
        let db = CustomFontDatabase::new();
        assert!(db.is_empty());
    }

    #[test]
    fn test_query_empty() {
        let db = CustomFontDatabase::new();
        let query = FontQuery::new(&["Arial"]);
        assert!(db.query(&query).is_none());
    }

    #[test]
    fn test_metadata_from_name_and_os2() {
        let data = font_bytes("Test Sans", 700, true, true);
        let faces = scan_faces(&mut data.as_slice());
        assert_eq!(faces.len(), 1);
        let meta = &faces[0].1;
        // Windows English record wins over the Mac record
        assert_eq!(meta.families, vec!["Test Sans".to_string()]);
        assert_eq!(meta.full_name, "Test Sans Full");
        assert_eq!(meta.weight, FontWeight(700));
        assert_eq!(meta.style, FontStyle::Italic);
        assert!(meta.has_glyf);
    }

    #[test]
    fn test_query_matches_family_weight_and_style() {
        let mut db = CustomFontDatabase::new();
        let regular = db.load_font_data(font_bytes("Test Sans", 400, false, true)).unwrap()[0];
        let bold = db.load_font_data(font_bytes("Test Sans", 700, false, true)).unwrap()[0];
        let italic = db.load_font_data(font_bytes("Test Sans", 400, true, true)).unwrap()[0];
        db.load_font_data(font_bytes("Other Serif", 400, false, true)).unwrap();

        // Family names match case-insensitively
        assert_eq!(db.query(&FontQuery::new(&["test sans"])), Some(regular));
        assert_eq!(db.query(&FontQuery::new(&["Test Sans"]).bold()), Some(bold));
        assert_eq!(db.query(&FontQuery::new(&["Test Sans"]).italic()), Some(italic));

        // Later families are tried when earlier ones are missing
        assert_eq!(db.query(&FontQuery::new(&["Missing", "Test Sans"])), Some(regular));

        // Face data is available for memory fonts
        assert!(db.with_face_data(regular, |data, index| data.len() > 0 && index == 0).unwrap());
    }

    #[test]
    fn test_fallback_prefers_outlines_we_can_rasterize() {
        let mut db = CustomFontDatabase::new();
        // A CFF-only face first in scan order must not become the fallback
        db.load_font_data(font_bytes("Aaa Sans", 400, false, false)).unwrap();
        db.load_font_data(font_bytes("Zzz Emoji", 400, false, true)).unwrap();
        let good = db.load_font_data(font_bytes("Good Sans", 400, false, true)).unwrap()[0];

        assert_eq!(db.query(&FontQuery::new(&["Nonexistent"])), Some(good));
    }

    #[test]
    fn test_font_file_is_scanned_without_loading_data() {
        let dir = std::env::temp_dir().join(format!("fos-font-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.ttf");
        std::fs::write(&path, font_bytes("File Sans", 400, false, true)).unwrap();

        let mut db = CustomFontDatabase::new();
        let id = db.load_font_file(&path).unwrap()[0];
        assert!(db.font(id).unwrap().data.get().is_none(), "data must load lazily");

        let len = db.with_face_data(id, |data, _| data.len()).unwrap();
        assert!(len > 0);
        assert!(db.font(id).unwrap().data.get().is_some());

        // File-backed faces are mapped, and read the same as the file
        let data = db.face_data(id).unwrap();
        assert!(data.is_mapped());
        assert_eq!(&*data, &std::fs::read(&path).unwrap()[..]);
        // In-memory faces are not
        let mem = db.load_font_data(font_bytes("Mem Sans", 400, false, true)).unwrap()[0];
        assert!(!db.face_data(mem).unwrap().is_mapped());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_garbage_is_rejected() {
        let mut db = CustomFontDatabase::new();
        assert!(db.load_font_data(vec![0; 64]).is_err());
        assert!(db.load_font_data(Vec::new()).is_err());
        // Truncated table directory
        let mut data = font_bytes("X", 400, false, true);
        data.truncate(20);
        assert!(db.load_font_data(data).is_err());
    }
}
