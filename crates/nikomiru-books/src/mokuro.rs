// SPDX-License-Identifier: GPL-3.0-only

//! Data-only Mokuro 0.2.x import (MNG-007…MNG-009).
//! Format inspected at kha-white/mokuro 9f79b1281066f953ec6e5d1e7086c401fa8da159,
//! mokuro_generator.py and volume.py; no upstream executable code is ported.
use crate::{Book, BookError, Chapter, Paragraph};
use serde::{
    Deserialize, Deserializer,
    de::{Error, SeqAccess, Visitor},
};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    io::{Cursor, Read},
    marker::PhantomData,
    path::{Component, Path, PathBuf},
};

/// Original clipped coordinates and stored reading text; no executable overlay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Region {
    /// Left, top, right, bottom in original image pixels.
    pub bounds: [u32; 4],
    /// Recognised text in original block/line order.
    pub text: String,
    /// Written top to bottom, its lines right to left.
    pub vertical: bool,
    /// Each line: its byte range in `text` and its box, from Mokuro's
    /// `lines_coords` or, without them, the block shared out evenly.
    pub lines: Vec<(std::ops::Range<usize>, [u32; 4])>,
}

impl Region {
    /// The byte of `text` under the point (`x`, `y`) of the original page,
    /// in pixels: the line under it (else the nearest), then the character
    /// at that share of the line's length (MNG-010).
    pub fn offset_at(&self, x: f32, y: f32) -> Option<usize> {
        let distance = |b: &[u32; 4]| {
            let dx = (b[0] as f32 - x).max(x - b[2] as f32).max(0.0);
            let dy = (b[1] as f32 - y).max(y - b[3] as f32).max(0.0);
            dx * dx + dy * dy
        };
        let (range, b) = self
            .lines
            .iter()
            .min_by(|a, b| distance(&a.1).total_cmp(&distance(&b.1)))?;
        let line = self.text.get(range.clone())?;
        let count = line.chars().count();
        if count == 0 {
            return None;
        }
        let share = if self.vertical {
            (y - b[1] as f32) / (b[3].saturating_sub(b[1]).max(1)) as f32
        } else {
            (x - b[0] as f32) / (b[2].saturating_sub(b[0]).max(1)) as f32
        };
        let index = ((share.clamp(0.0, 0.999) * count as f32) as usize).min(count - 1);
        let within = line.char_indices().nth(index).map_or(0, |(i, _)| i);
        Some(range.start + within)
    }
}
/// Text regions and verified original page dimensions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    /// Width of the decoded page.
    pub width: u32,
    /// Height of the decoded page.
    pub height: u32,
    /// Original ordered regions.
    pub regions: Vec<Region>,
}
impl Page {
    /// Normalised crop shares for the existing asynchronous image worker.
    pub fn area(&self, index: usize) -> Option<[f32; 4]> {
        let [l, t, r, b] = self.regions.get(index)?.bounds;
        Some([
            l as f32 / self.width as f32,
            t as f32 / self.height as f32,
            (r - l) as f32 / self.width as f32,
            (b - t) as f32 / self.height as f32,
        ])
    }
}
fn invalid() -> BookError {
    BookError::InvalidMokuro
}
fn limited<'de, D, T, const N: usize>(d: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct V<T, const N: usize>(PhantomData<T>);
    impl<'de, T: Deserialize<'de>, const N: usize> Visitor<'de> for V<T, N> {
        type Value = Vec<T>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            write!(f, "at most {N} entries")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            let mut out = Vec::new();
            while let Some(v) = seq.next_element()? {
                if out.len() == N {
                    return Err(A::Error::custom("Mokuro entry limit"));
                }
                out.push(v);
            }
            Ok(out)
        }
    }
    d.deserialize_seq(V::<T, N>(PhantomData))
}
#[derive(Deserialize)]
struct Volume {
    version: String,
    title: String,
    #[serde(default)]
    volume: String,
    #[serde(deserialize_with = "limited::<_,_,1000>")]
    pages: Vec<InputPage>,
}
#[derive(Deserialize)]
struct InputPage {
    img_path: String,
    img_width: u32,
    img_height: u32,
    #[serde(deserialize_with = "limited::<_,_,1000>")]
    blocks: Vec<Block>,
}
#[derive(Deserialize)]
struct Block {
    #[serde(rename = "box")]
    bounds: [f64; 4],
    #[serde(default)]
    vertical: bool,
    #[serde(deserialize_with = "limited::<_,_,256>")]
    lines: Vec<String>,
    /// Each line's four corners, when Mokuro wrote them.
    #[serde(default, deserialize_with = "limited::<_,_,256>")]
    lines_coords: Vec<[[f64; 2]; 4]>,
}

/// The lines' ranges in the joined text and their boxes, in page pixels.
fn line_boxes(
    block: &Block,
    bounds: [u32; 4],
    size: (f64, f64),
) -> Vec<(std::ops::Range<usize>, [u32; 4])> {
    let n = block.lines.len();
    let joint = usize::from(!block.vertical);
    let mut at = 0;
    let mut out = Vec::with_capacity(n);
    let [l, t, r, b] = bounds.map(f64::from);
    for (i, line) in block.lines.iter().enumerate() {
        let range = at..at + line.len();
        at = range.end + joint;
        let given = block
            .lines_coords
            .get(i)
            .filter(|_| block.lines_coords.len() == n)
            .filter(|q| q.iter().flatten().all(|v| v.is_finite()))
            .map(|q| {
                let xs = q.iter().map(|p| p[0]);
                let ys = q.iter().map(|p| p[1]);
                let (x0, x1) = (
                    xs.clone().fold(f64::MAX, f64::min),
                    xs.fold(f64::MIN, f64::max),
                );
                let (y0, y1) = (
                    ys.clone().fold(f64::MAX, f64::min),
                    ys.fold(f64::MIN, f64::max),
                );
                [x0, y0, x1, y1]
            });
        let [x0, y0, x1, y1] = given.unwrap_or_else(|| {
            let part = i as f64 / n.max(1) as f64;
            let next = (i + 1) as f64 / n.max(1) as f64;
            if block.vertical {
                // columns, the first on the right
                [r - (r - l) * next, t, r - (r - l) * part, b]
            } else {
                [l, t + (b - t) * part, r, t + (b - t) * next]
            }
        });
        let clip = |v: f64, max: f64| v.clamp(0.0, max).round() as u32;
        out.push((
            range,
            [
                clip(x0, size.0),
                clip(y0, size.1),
                clip(x1, size.0),
                clip(y1, size.1),
            ],
        ));
    }
    out
}
fn bytes(path: &Path, limit: u64) -> Result<Vec<u8>, BookError> {
    if !std::fs::metadata(path)
        .map_err(|e| BookError::Io(e.to_string()))?
        .is_file()
    {
        return Err(invalid());
    }
    let mut out = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| BookError::Io(e.to_string()))?
        .take(limit + 1)
        .read_to_end(&mut out)
        .map_err(|e| BookError::Io(e.to_string()))?;
    if out.len() as u64 > limit {
        return Err(BookError::TooLarge);
    }
    Ok(out)
}
fn local(root: &Path, name: &str) -> Result<PathBuf, BookError> {
    if name.is_empty()
        || name.len() > 4096
        || name.contains(['\\', ':'])
        || name.chars().any(char::is_control)
        || Path::new(name)
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(invalid());
    }
    let path = std::fs::canonicalize(root.join(name)).map_err(|e| BookError::Io(e.to_string()))?;
    if !path.starts_with(root) {
        return Err(invalid());
    }
    Ok(path)
}
fn regions(page: &InputPage) -> Result<Vec<Region>, BookError> {
    let mut out = Vec::new();
    for block in &page.blocks {
        let [l, t, r, b] = block.bounds;
        let w = f64::from(page.img_width);
        let h = f64::from(page.img_height);
        if block.bounds.iter().any(|v| !v.is_finite())
            || r <= l
            || b <= t
            || l < -w
            || t < -h
            || r > 2.0 * w
            || b > 2.0 * h
        {
            return Err(invalid());
        }
        let bounds = [
            l.clamp(0.0, w).floor() as u32,
            t.clamp(0.0, h).floor() as u32,
            r.clamp(0.0, w).ceil() as u32,
            b.clamp(0.0, h).ceil() as u32,
        ];
        if bounds[2].saturating_sub(bounds[0]) < 8 || bounds[3].saturating_sub(bounds[1]) < 8 {
            return Err(invalid());
        }
        if block
            .lines
            .iter()
            .any(|s| s.len() > 65536 || s.chars().any(|c| c.is_control() && c != '\n'))
        {
            return Err(invalid());
        }
        let text = block.lines.join(if block.vertical { "" } else { "\n" });
        if text.len() > 65536 {
            return Err(BookError::TooLarge);
        }
        if !text.trim().is_empty() {
            let lines = line_boxes(block, bounds, (w, h));
            out.push(Region {
                bounds,
                text,
                vertical: block.vertical,
                lines,
            });
        }
    }
    Ok(out)
}
/// Validate an entire local volume before returning any image/text to the reader.
pub fn read(path: &Path) -> Result<Book, BookError> {
    let manifest = bytes(path, 32 << 20)?;
    let volume: Volume = serde_json::from_slice(&manifest).map_err(|_| invalid())?;
    if !volume.version.starts_with("0.2.")
        || volume.version.len() > 64
        || volume.pages.is_empty()
        || volume.title.trim().is_empty()
        || volume.title.chars().count() > 256
        || volume.volume.chars().count() > 256
        || volume
            .title
            .chars()
            .chain(volume.volume.chars())
            .any(char::is_control)
    {
        return Err(invalid());
    }
    let folder = path.with_extension("");
    let folder = if folder.is_dir() {
        folder
    } else {
        path.parent().ok_or_else(invalid)?.into()
    };
    if std::fs::symlink_metadata(&folder)
        .map_err(|e| BookError::Io(e.to_string()))?
        .file_type()
        .is_symlink()
    {
        return Err(invalid());
    }
    let root = std::fs::canonicalize(folder).map_err(|e| BookError::Io(e.to_string()))?;
    let mut images = BTreeMap::new();
    let mut pages = BTreeMap::new();
    let mut chapters = Vec::new();
    let mut seen = HashSet::new();
    let mut total = 0usize;
    let mut text_bytes = 0usize;
    let mut count = 0usize;
    let mut hash = Sha256::new();
    hash.update(&manifest);
    for (i, page) in volume.pages.into_iter().enumerate() {
        if page.img_width == 0
            || page.img_height == 0
            || page.img_width > 16384
            || page.img_height > 16384
            || u64::from(page.img_width) * u64::from(page.img_height) > 32_000_000
        {
            return Err(invalid());
        }
        let target = local(&root, &page.img_path)?;
        if !seen.insert(target.clone()) {
            return Err(invalid());
        }
        let image = bytes(&target, 32 << 20)?;
        total = total.saturating_add(image.len());
        if total > 512 << 20 {
            return Err(BookError::TooLarge);
        }
        let picture = || -> Result<(), BookError> {
            let mut reader = image::ImageReader::new(Cursor::new(&image))
                .with_guessed_format()
                .map_err(|_| invalid())?;
            let mut limits = image::Limits::default();
            limits.max_image_width = Some(16384);
            limits.max_image_height = Some(16384);
            limits.max_alloc = Some(128 << 20);
            reader.limits(limits);
            let decoded = reader.decode().map_err(|_| invalid())?;
            if decoded.width() != page.img_width || decoded.height() != page.img_height {
                return Err(invalid());
            }
            Ok(())
        };
        picture()?;
        let blocks = regions(&page)?;
        count += blocks.len();
        text_bytes += blocks.iter().map(|r| r.text.len()).sum::<usize>();
        if count > 100000 || text_bytes > 16 << 20 {
            return Err(BookError::TooLarge);
        }
        hash.update((image.len() as u64).to_le_bytes());
        hash.update(&image);
        let key = format!("mokuro/{i}");
        images.insert(key.clone(), image);
        chapters.push(Chapter {
            paragraphs: vec![Paragraph {
                image: Some(key),
                ..Default::default()
            }],
            ..Default::default()
        });
        pages.insert(
            i,
            Page {
                width: page.img_width,
                height: page.img_height,
                regions: blocks,
            },
        );
    }
    let identity = hash
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let title = if volume.volume.is_empty() {
        volume.title
    } else {
        format!("{} · {}", volume.title, volume.volume)
    };
    Ok(Book {
        identity: Some(format!("mokuro:{identity}")),
        title,
        right_to_left: true,
        manga: true,
        manga_regions: pages,
        chapters,
        images,
        ..Default::default()
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(root: &Path) -> PathBuf {
        let path = root.join("Volume.mokuro");
        std::fs::create_dir(root.join("Volume")).unwrap();
        image::RgbImage::new(100, 200)
            .save(root.join("Volume/1.png"))
            .unwrap();
        std::fs::write(&path,serde_json::to_vec(&serde_json::json!({"version":"0.2.0","title":"漫画","volume":"一","pages":[{"img_path":"1.png","img_width":100,"img_height":200,"blocks":[{"box":[-2,10,80,100],"vertical":true,"lines":["猫","です"]}]}]})).unwrap()).unwrap();
        path
    }
    // T-MNG-007, T-MNG-008: imported order/text/geometry and content identity retain local sources without OCR.
    #[test]
    fn volume_preserves_text_and_clips_real_ocr_edge_boxes() {
        let dir = tempfile::tempdir().unwrap();
        let path = fixture(dir.path());
        let b = read(&path).unwrap();
        assert!(b.manga);
        assert_eq!(b.chars(), 1);
        assert_eq!(b.manga_regions[&0].regions[0].text, "猫です");
        assert_eq!(b.manga_regions[&0].regions[0].bounds, [0, 10, 80, 100]);
        assert_eq!(b.manga_regions[&0].area(0), Some([0.0, 0.05, 0.8, 0.45]));
        let moved = dir.path().join("moved");
        std::fs::create_dir(&moved).unwrap();
        std::fs::rename(&path, moved.join("Volume.mokuro")).unwrap();
        std::fs::rename(dir.path().join("Volume"), moved.join("Volume")).unwrap();
        assert_eq!(
            read(&moved.join("Volume.mokuro")).unwrap().identity,
            b.identity
        );
    }
    // T-MNG-010: a point of the page names the character under it, from
    // Mokuro's line boxes or the block shared out
    #[test]
    fn a_point_names_its_character() {
        let dir = tempfile::tempdir().unwrap();
        let path = fixture(dir.path());
        let b = read(&path).unwrap();
        let region = &b.manga_regions[&0].regions[0];
        // two columns over [0,10,80,100]: 猫 on the right, です on the left
        assert!(region.vertical);
        assert_eq!(region.offset_at(70.0, 50.0), Some(0));
        assert_eq!(region.offset_at(20.0, 20.0), Some(3));
        assert_eq!(region.offset_at(20.0, 90.0), Some(6));
        // given line boxes, horizontal text
        let mut value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        value["pages"][0]["blocks"][0] = serde_json::json!({
            "box": [0, 0, 100, 40], "vertical": false, "lines": ["猫です", "犬"],
            "lines_coords": [[[0,0],[60,0],[60,20],[0,20]], [[0,20],[20,20],[20,40],[0,40]]],
        });
        std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        let b = read(&path).unwrap();
        let region = &b.manga_regions[&0].regions[0];
        assert_eq!(region.text, "猫です\n犬");
        assert_eq!(region.offset_at(5.0, 5.0), Some(0));
        assert_eq!(region.offset_at(25.0, 5.0), Some(3));
        assert_eq!(region.offset_at(55.0, 5.0), Some(6));
        // below the first line, nearest the second
        assert_eq!(region.offset_at(25.0, 35.0), Some(10));
    }

    // T-MNG-009: unsafe paths, bad coordinates/versions/dimensions, duplicate and missing images reject atomically.
    #[test]
    fn malformed_or_nonlocal_volumes_reject_as_a_whole() {
        let dir = tempfile::tempdir().unwrap();
        let path = fixture(dir.path());
        let good: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        for name in [
            "../1.png",
            "/tmp/1.png",
            "https://x/a.png",
            "C:\\x.png",
            "missing.png",
        ] {
            let mut bad = good.clone();
            bad["pages"][0]["img_path"] = name.into();
            std::fs::write(&path, serde_json::to_vec(&bad).unwrap()).unwrap();
            assert!(read(&path).is_err());
        }
        for (field, value) in [
            ("img_width", serde_json::json!(99)),
            ("img_height", serde_json::json!(0)),
            (
                "blocks",
                serde_json::json!([{"box":[10,10,2,2],"lines":["猫"]}]),
            ),
        ] {
            let mut bad = good.clone();
            bad["pages"][0][field] = value;
            std::fs::write(&path, serde_json::to_vec(&bad).unwrap()).unwrap();
            assert!(read(&path).is_err());
        }
        let mut bad = good.clone();
        bad["pages"]
            .as_array_mut()
            .unwrap()
            .push(good["pages"][0].clone());
        std::fs::write(&path, serde_json::to_vec(&bad).unwrap()).unwrap();
        assert!(read(&path).is_err());
        let mut bad = good;
        bad["version"] = "9.0".into();
        std::fs::write(&path, serde_json::to_vec(&bad).unwrap()).unwrap();
        assert!(read(&path).is_err());
    }
    // T-MNG-009: a manifest cannot follow an escaping symlink.
    #[cfg(unix)]
    #[test]
    fn symlink_escape_is_not_a_volume_image() {
        let dir = tempfile::tempdir().unwrap();
        let path = fixture(dir.path());
        let outside = tempfile::tempdir().unwrap();
        std::fs::rename(
            dir.path().join("Volume/1.png"),
            outside.path().join("1.png"),
        )
        .unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("1.png"),
            dir.path().join("Volume/1.png"),
        )
        .unwrap();
        assert!(read(&path).is_err());
    }
}
