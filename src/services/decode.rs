// In-process decoders for book/comic formats (zero host exec). This first P2 increment covers
// CBZ (zip archives of images): page listing in natural order, single-page extraction, and JPEG
// thumbnail generation. CB7/CBR/PDF/EPUB plug into the same `list_pages` / `read_page` dispatch.
use std::fs::File;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use image::{imageops::FilterType, DynamicImage, ImageFormat, ImageReader};
use uuid::Uuid;

use crate::{config::Settings, errors::BooksError};

/// Canonical local-storage prefix. A path `[Drive]/<rel>` denotes the requesting
/// user's own Drive root, i.e. `<owner_id>/files/<rel>` under the shared store.
/// The owner UUID is implicit (resolved from the row's owner_id), never written.
pub const DRIVE_PREFIX: &str = "[Drive]/";

const IMG_EXTS: &[&str] = &["jpg", "jpeg", "png", "webp", "gif", "bmp", "jfif"];

/// Formats whose pages are raster images packed in an archive.
pub fn is_image_archive(format: &str) -> bool {
    matches!(format, "cbz" | "cb7" | "cbr")
}

/// Resolve a drive-relative storage path to an absolute filesystem path under the shared store.
pub fn physical_path(settings: &Settings, storage_path: &str) -> Result<PathBuf, BooksError> {
    let base = settings
        .storage
        .files_storage_base
        .as_deref()
        .ok_or_else(|| BooksError::Validation("storage.files_storage_base non configuré".into()))?;
    Ok(Path::new(base.trim_end_matches('/')).join(storage_path.trim_start_matches('/')))
}

/// Resolve a canonical `storage_path` to a readable filesystem path.
///
/// Resolution order:
///  1. `local_cache_path` (remote-mount files cached locally) wins outright.
///  2. `[Drive]/<rel>` → `<files_storage_base>/<owner_id>/files/<rel>`.
///  3. Any other value (legacy raw drive path or a `filesystem` absolute path) is
///     joined onto the shared store as-is for backward compatibility.
pub fn resolve_storage(
    settings:         &Settings,
    storage_path:     &str,
    local_cache_path: Option<&str>,
    owner_id:         Uuid,
) -> Result<PathBuf, BooksError> {
    if let Some(cache) = local_cache_path.filter(|c| !c.is_empty()) {
        return Ok(PathBuf::from(cache));
    }
    if let Some(rel) = storage_path.strip_prefix(DRIVE_PREFIX) {
        let base = settings
            .storage
            .files_storage_base
            .as_deref()
            .ok_or_else(|| BooksError::Validation("storage.files_storage_base non configuré".into()))?;
        return Ok(Path::new(base.trim_end_matches('/'))
            .join(owner_id.to_string())
            .join("files")
            .join(rel.trim_start_matches('/')));
    }
    physical_path(settings, storage_path)
}

fn is_image_name(name: &str) -> bool {
    if name.ends_with('/') {
        return false;
    }
    let lower = name.to_lowercase();
    IMG_EXTS.iter().any(|e| lower.ends_with(&format!(".{e}")))
}

fn content_type_for(name: &str) -> String {
    let l = name.to_lowercase();
    if l.ends_with(".png") {
        "image/png"
    } else if l.ends_with(".webp") {
        "image/webp"
    } else if l.ends_with(".gif") {
        "image/gif"
    } else if l.ends_with(".bmp") {
        "image/bmp"
    } else {
        "image/jpeg"
    }
    .to_string()
}

/// Ordered list of page entry names inside a book file.
pub fn list_pages(format: &str, path: &Path) -> Result<Vec<String>, BooksError> {
    match format {
        "cbz" => list_pages_zip(path),
        "cb7" => list_pages_7z(path),
        "cbr" => list_pages_rar(path),
        other => Err(BooksError::Validation(format!("Format sans pages-image: {other}"))),
    }
}

fn natsort(names: &mut [String]) {
    names.sort_by(|a, b| natord::compare(&a.to_lowercase(), &b.to_lowercase()));
}

fn list_pages_zip(path: &Path) -> Result<Vec<String>, BooksError> {
    let file = File::open(path).map_err(|e| BooksError::Storage(e.to_string()))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| BooksError::Storage(e.to_string()))?;
    let mut names: Vec<String> = Vec::new();
    for i in 0..zip.len() {
        let f = zip.by_index(i).map_err(|e| BooksError::Storage(e.to_string()))?;
        if f.is_file() && is_image_name(f.name()) {
            names.push(f.name().to_string());
        }
    }
    names.sort_by(|a, b| natord::compare(&a.to_lowercase(), &b.to_lowercase()));
    Ok(names)
}

/// Raw bytes of one page entry + a guessed content-type.
pub fn read_page(format: &str, path: &Path, entry: &str) -> Result<(Vec<u8>, String), BooksError> {
    match format {
        "cbz" => read_entry_zip(path, entry),
        "cb7" => read_entry_7z(path, entry),
        "cbr" => read_entry_rar(path, entry),
        other => Err(BooksError::Validation(format!("Format non pris en charge: {other}"))),
    }
}

// ── CB7 (7-zip archives) ─────────────────────────────────────────────────────────
fn list_pages_7z(path: &Path) -> Result<Vec<String>, BooksError> {
    let mut names = Vec::new();
    let mut reader = sevenz_rust::SevenZReader::open(path, sevenz_rust::Password::empty())
        .map_err(|e| BooksError::Storage(e.to_string()))?;
    reader
        .for_each_entries(|entry, _r| {
            if !entry.is_directory && is_image_name(&entry.name) {
                names.push(entry.name.clone());
            }
            Ok(true)
        })
        .map_err(|e| BooksError::Storage(e.to_string()))?;
    natsort(&mut names);
    Ok(names)
}

fn read_entry_7z(path: &Path, entry: &str) -> Result<(Vec<u8>, String), BooksError> {
    let mut out: Option<Vec<u8>> = None;
    let mut reader = sevenz_rust::SevenZReader::open(path, sevenz_rust::Password::empty())
        .map_err(|e| BooksError::Storage(e.to_string()))?;
    // 7z archives are typically solid: each entry's stream must be consumed in order to advance
    // the decompressor, so we read every entry and keep the one we want.
    reader
        .for_each_entries(|e, r| {
            if !e.is_directory {
                let mut buf = Vec::new();
                let _ = std::io::Read::read_to_end(r, &mut buf);
                if e.name == entry {
                    out = Some(buf);
                }
            }
            Ok(true)
        })
        .map_err(|e| BooksError::Storage(e.to_string()))?;
    out.map(|b| (b, content_type_for(entry)))
        .ok_or_else(|| BooksError::NotFound(format!("Entrée {entry}")))
}

// ── CBR (RAR archives) ───────────────────────────────────────────────────────────
fn list_pages_rar(path: &Path) -> Result<Vec<String>, BooksError> {
    let mut names = Vec::new();
    let list = unrar::Archive::new(path)
        .open_for_listing()
        .map_err(|e| BooksError::Storage(format!("{e:?}")))?;
    for entry in list {
        let e = entry.map_err(|e| BooksError::Storage(format!("{e:?}")))?;
        let name = e.filename.to_string_lossy().replace('\\', "/");
        if e.is_file() && is_image_name(&name) {
            names.push(name);
        }
    }
    natsort(&mut names);
    Ok(names)
}

fn read_entry_rar(path: &Path, entry: &str) -> Result<(Vec<u8>, String), BooksError> {
    let mut arc = unrar::Archive::new(path)
        .open_for_processing()
        .map_err(|e| BooksError::Storage(format!("{e:?}")))?;
    loop {
        let header = match arc.read_header().map_err(|e| BooksError::Storage(format!("{e:?}")))? {
            Some(h) => h,
            None => break,
        };
        let name = header.entry().filename.to_string_lossy().replace('\\', "/");
        if name == entry {
            let (data, _next) = header.read().map_err(|e| BooksError::Storage(format!("{e:?}")))?;
            return Ok((data, content_type_for(entry)));
        }
        arc = header.skip().map_err(|e| BooksError::Storage(format!("{e:?}")))?;
    }
    Err(BooksError::NotFound(format!("Entrée {entry}")))
}

// ── EPUB (zip + OPF) — extract the cover image ───────────────────────────────────
fn normalize_zip_path(p: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for seg in p.split('/') {
        match seg {
            "" | "." => {}
            ".." => { out.pop(); }
            s => out.push(s),
        }
    }
    out.join("/")
}

pub fn epub_cover(path: &Path) -> Result<(Vec<u8>, String), BooksError> {
    let file = File::open(path).map_err(|e| BooksError::Storage(e.to_string()))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| BooksError::Storage(e.to_string()))?;

    // 1. container.xml → OPF path
    let opf_path = {
        let mut s = String::new();
        zip.by_name("META-INF/container.xml")
            .map_err(|_| BooksError::NotFound("container.xml".into()))?
            .read_to_string(&mut s)
            .ok();
        let doc = roxmltree::Document::parse(&s).map_err(|e| BooksError::Storage(e.to_string()))?;
        doc.descendants()
            .find(|n| n.has_tag_name("rootfile"))
            .and_then(|n| n.attribute("full-path"))
            .map(|s| s.to_string())
            .ok_or_else(|| BooksError::NotFound("rootfile".into()))?
    };

    // 2. OPF → cover image href
    let opf_dir = opf_path.rsplit_once('/').map(|(d, _)| d.to_string()).unwrap_or_default();
    let opf = {
        let mut s = String::new();
        zip.by_name(&opf_path)
            .map_err(|_| BooksError::NotFound("opf".into()))?
            .read_to_string(&mut s)
            .ok();
        s
    };
    let href = find_epub_cover_href(&opf)
        .ok_or_else(|| BooksError::NotFound("Couverture EPUB absente".into()))?;
    let full = normalize_zip_path(&if opf_dir.is_empty() { href } else { format!("{opf_dir}/{href}") });

    // 3. extract the image bytes
    let mut buf = Vec::new();
    zip.by_name(&full)
        .map_err(|_| BooksError::NotFound(format!("Image {full}")))?
        .read_to_end(&mut buf)
        .map_err(|e| BooksError::Storage(e.to_string()))?;
    Ok((buf, content_type_for(&full)))
}

fn find_epub_cover_href(opf: &str) -> Option<String> {
    let doc = roxmltree::Document::parse(opf).ok()?;
    // EPUB3: <item properties="cover-image" href="..."/>
    if let Some(item) = doc.descendants().find(|n| {
        n.has_tag_name("item")
            && n.attribute("properties").is_some_and(|p| p.split_whitespace().any(|x| x == "cover-image"))
    }) {
        return item.attribute("href").map(|s| s.to_string());
    }
    // EPUB2: <meta name="cover" content="ID"/> → <item id="ID" href="..."/>
    let cover_id = doc
        .descendants()
        .find(|n| n.has_tag_name("meta") && n.attribute("name") == Some("cover"))
        .and_then(|n| n.attribute("content"))?;
    let item = doc
        .descendants()
        .find(|n| n.has_tag_name("item") && n.attribute("id") == Some(cover_id))?;
    item.attribute("href").map(|s| s.to_string())
}

// ── PDF — page count (rendering is done client-side by the reader) ───────────────
pub fn pdf_page_count(path: &Path) -> Result<usize, BooksError> {
    let doc = lopdf::Document::load(path).map_err(|e| BooksError::Storage(e.to_string()))?;
    Ok(doc.get_pages().len())
}

// ── Embedded metadata (ComicInfo.xml for archives, OPF for EPUB) ──────────────────
#[derive(Debug, Default)]
pub struct EmbeddedMeta {
    pub title:       Option<String>,
    pub series:      Option<String>,
    pub number:      Option<f64>,
    pub authors:     Vec<(String, String)>, // (name, role)
    pub publisher:   Option<String>,
    pub date:        Option<NaiveDate>,
    pub description: Option<String>,
    pub tags:        Vec<String>,
    pub language:    Option<String>,
    pub isbn:        Option<String>,
    pub rtl:         bool,
}

/// Extract embedded metadata for a format (best-effort).
pub fn embedded_meta(format: &str, path: &Path) -> Option<EmbeddedMeta> {
    match format {
        "cbz" | "cb7" | "cbr" => comicinfo(format, path),
        "epub" => epub_meta(path),
        _ => None,
    }
}

fn split_csv(s: &str) -> Vec<String> {
    s.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect()
}

fn comicinfo(format: &str, path: &Path) -> Option<EmbeddedMeta> {
    let xml = ["ComicInfo.xml", "comicinfo.xml"]
        .iter()
        .find_map(|n| read_page(format, path, n).ok().map(|(b, _)| b))?;
    let s = String::from_utf8_lossy(&xml);
    let doc = roxmltree::Document::parse(&s).ok()?;
    let root = doc.root_element();
    let get = |tag: &str| {
        root.children()
            .find(|n| n.has_tag_name(tag))
            .and_then(|n| n.text())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    };

    let mut m = EmbeddedMeta {
        title: get("Title"),
        series: get("Series"),
        number: get("Number").and_then(|s| s.parse().ok()),
        publisher: get("Publisher"),
        description: get("Summary"),
        language: get("LanguageISO"),
        tags: get("Genre").map(|g| split_csv(&g)).unwrap_or_default(),
        rtl: get("Manga").map(|s| s.eq_ignore_ascii_case("YesAndRightToLeft")).unwrap_or(false),
        ..Default::default()
    };
    for (tag, role) in [
        ("Writer", "writer"), ("Penciller", "penciller"), ("Inker", "inker"),
        ("Colorist", "colorist"), ("Letterer", "letterer"), ("CoverArtist", "cover"),
        ("Editor", "editor"),
    ] {
        if let Some(v) = get(tag) {
            for name in split_csv(&v) {
                m.authors.push((name, role.to_string()));
            }
        }
    }
    if let Some(y) = get("Year").and_then(|s| s.parse::<i32>().ok()) {
        let mo = get("Month").and_then(|s| s.parse::<u32>().ok()).unwrap_or(1).clamp(1, 12);
        let d = get("Day").and_then(|s| s.parse::<u32>().ok()).unwrap_or(1).clamp(1, 31);
        m.date = NaiveDate::from_ymd_opt(y, mo, d).or_else(|| NaiveDate::from_ymd_opt(y, mo, 1));
    }
    Some(m)
}

fn parse_loose_date(s: &str) -> Option<NaiveDate> {
    let s = s.trim();
    if let Ok(d) = NaiveDate::parse_from_str(&s[..s.len().min(10)], "%Y-%m-%d") {
        return Some(d);
    }
    s.get(..4).and_then(|y| y.parse::<i32>().ok()).and_then(|y| NaiveDate::from_ymd_opt(y, 1, 1))
}

fn epub_meta(path: &Path) -> Option<EmbeddedMeta> {
    let file = File::open(path).ok()?;
    let mut zip = zip::ZipArchive::new(file).ok()?;
    let opf_path = {
        let mut s = String::new();
        zip.by_name("META-INF/container.xml").ok()?.read_to_string(&mut s).ok()?;
        let doc = roxmltree::Document::parse(&s).ok()?;
        doc.descendants()
            .find(|n| n.has_tag_name("rootfile"))
            .and_then(|n| n.attribute("full-path"))?
            .to_string()
    };
    let opf = {
        let mut s = String::new();
        zip.by_name(&opf_path).ok()?.read_to_string(&mut s).ok()?;
        s
    };
    let doc = roxmltree::Document::parse(&opf).ok()?;
    // Match Dublin Core elements by local name (ignore namespace prefix).
    let txt = |tag: &str| {
        doc.descendants()
            .find(|n| n.is_element() && n.tag_name().name() == tag)
            .and_then(|n| n.text())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    };
    let all = |tag: &str| {
        doc.descendants()
            .filter(|n| n.is_element() && n.tag_name().name() == tag)
            .filter_map(|n| n.text())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
    };
    let isbn = doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "identifier")
        .filter_map(|n| n.text())
        .map(|s| s.trim().to_string())
        .find(|s| {
            let l = s.to_lowercase();
            l.contains("isbn") || s.chars().filter(|c| c.is_ascii_digit()).count() >= 10
        })
        .map(|s| s.to_lowercase().replace("urn:isbn:", "").replace("isbn:", "").trim().to_string());

    Some(EmbeddedMeta {
        title: txt("title"),
        authors: all("creator").into_iter().map(|n| (n, "author".to_string())).collect(),
        publisher: txt("publisher"),
        description: txt("description"),
        language: txt("language"),
        tags: all("subject"),
        date: txt("date").and_then(|d| parse_loose_date(&d)),
        isbn,
        ..Default::default()
    })
}

fn read_entry_zip(path: &Path, entry: &str) -> Result<(Vec<u8>, String), BooksError> {
    let file = File::open(path).map_err(|e| BooksError::Storage(e.to_string()))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| BooksError::Storage(e.to_string()))?;
    let mut zf = zip
        .by_name(entry)
        .map_err(|_| BooksError::NotFound(format!("Entrée {entry}")))?;
    let mut buf = Vec::with_capacity(zf.size() as usize);
    zf.read_to_end(&mut buf).map_err(|e| BooksError::Storage(e.to_string()))?;
    Ok((buf, content_type_for(entry)))
}

/// Cheap image dimensions from a header (no full decode).
pub fn image_dims(bytes: &[u8]) -> Option<(u32, u32)> {
    ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()
}

/// JPEG thumbnail that fits within `max_w` (keeps aspect ratio).
pub fn thumbnail(bytes: &[u8], max_w: u32) -> Result<Vec<u8>, BooksError> {
    let img = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| BooksError::Storage(e.to_string()))?
        .decode()
        .map_err(|e| BooksError::Storage(e.to_string()))?;
    let resized = if img.width() > max_w {
        let h = ((img.height() as f32) * (max_w as f32 / img.width() as f32)).round() as u32;
        img.resize(max_w, h.max(1), FilterType::Lanczos3)
    } else {
        img
    };
    let rgb = DynamicImage::ImageRgb8(resized.to_rgb8());
    let mut out = Vec::new();
    rgb.write_to(&mut Cursor::new(&mut out), ImageFormat::Jpeg)
        .map_err(|e| BooksError::Storage(e.to_string()))?;
    Ok(out)
}
