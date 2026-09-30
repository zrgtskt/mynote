//! 画像ファイルの取り込み（保存・サムネイル作成）と、Claude に送るための縮小

use std::io::Cursor;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use image::codecs::jpeg::JpegEncoder;
use image::metadata::Orientation;
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader};
use sha2::{Digest, Sha256};

/// 取り込める画像の最大サイズ
pub const MAX_IMPORT_BYTES: usize = 50 * 1024 * 1024;
/// 一覧に出すサムネイルの長辺
const THUMB_EDGE: u32 = 720;
/// Claude に送るときの長辺。これより大きい画像は縮小して費用を抑える（文字が読める程度は残す）
pub const CLAUDE_EDGE: u32 = 1600;
/// Claude に送る画像の最大バイト数（base64 にしても 5MB 未満に収める）
const CLAUDE_MAX_BYTES: usize = 3_600_000;

pub const UNSUPPORTED: &str =
    "対応している画像形式は JPEG・PNG・GIF・WebP です（HEIC などは写真アプリで JPEG に書き出してから取り込んでください）";

/// 保存した画像の情報
#[derive(Debug, Clone)]
pub struct Stored {
    pub hash: String,
    pub file_path: PathBuf,
    pub thumb_path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub mime: &'static str,
    pub size: usize,
}

/// 画像の形式を中身から判定する（拡張子は信用しない）
pub fn detect(bytes: &[u8]) -> Result<(ImageFormat, &'static str, &'static str)> {
    match image::guess_format(bytes) {
        Ok(ImageFormat::Jpeg) => Ok((ImageFormat::Jpeg, "image/jpeg", "jpg")),
        Ok(ImageFormat::Png) => Ok((ImageFormat::Png, "image/png", "png")),
        Ok(ImageFormat::Gif) => Ok((ImageFormat::Gif, "image/gif", "gif")),
        Ok(ImageFormat::WebP) => Ok((ImageFormat::WebP, "image/webp", "webp")),
        _ => bail!(UNSUPPORTED),
    }
}

/// 中身のハッシュ（同じ画像を二重に取り込まないために使う）
pub fn content_hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .take(16)
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// 写真の向き（EXIF）を反映して読み込む
fn decode_oriented(bytes: &[u8]) -> Result<(DynamicImage, Orientation)> {
    let reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    let mut decoder = reader.into_decoder().context("画像を読み込めません")?;
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let mut img =
        DynamicImage::from_decoder(decoder).context("画像を読み込めません（ファイルが壊れている可能性があります）")?;
    img.apply_orientation(orientation);
    Ok((img, orientation))
}

/// 透過があれば PNG、なければ JPEG にする
fn encode(img: &DynamicImage, quality: u8) -> Result<(Vec<u8>, &'static str, &'static str)> {
    let mut buf = Vec::new();
    if img.has_alpha() {
        img.write_to(&mut Cursor::new(&mut buf), ImageFormat::Png)?;
        Ok((buf, "image/png", "png"))
    } else {
        JpegEncoder::new_with_quality(&mut buf, quality).encode_image(&img.to_rgb8())?;
        Ok((buf, "image/jpeg", "jpg"))
    }
}

fn fit_within(img: &DynamicImage, edge: u32) -> DynamicImage {
    if img.width().max(img.height()) <= edge {
        img.clone()
    } else {
        img.resize(edge, edge, image::imageops::FilterType::Triangle)
    }
}

/// 画像を dir/ に保存し、サムネイルを dir/thumbs/ に作る
pub fn store(bytes: &[u8], dir: &Path) -> Result<Stored> {
    if bytes.len() > MAX_IMPORT_BYTES {
        bail!("画像が大きすぎます（{} MB まで）", MAX_IMPORT_BYTES / 1024 / 1024);
    }
    let (_, mime, ext) = detect(bytes)?;
    let hash = content_hash(bytes);
    let thumbs = dir.join("thumbs");
    std::fs::create_dir_all(&thumbs)?;

    let file_path = dir.join(format!("{hash}.{ext}"));
    if !file_path.exists() {
        std::fs::write(&file_path, bytes).with_context(|| format!("画像を保存できません: {}", file_path.display()))?;
    }

    let (img, _) = decode_oriented(bytes)?;
    let (width, height) = (img.width(), img.height());
    let (thumb, _, thumb_ext) = encode(&fit_within(&img, THUMB_EDGE), 82)?;
    let thumb_path = thumbs.join(format!("{hash}.{thumb_ext}"));
    std::fs::write(&thumb_path, thumb)?;

    Ok(Stored {
        hash,
        file_path,
        thumb_path,
        width,
        height,
        mime,
        size: bytes.len(),
    })
}

/// Claude に送る形（media_type と base64）にする。大きい画像や向きの補正が必要な写真は作り直す。
pub fn for_claude(path: &Path) -> Result<(String, String)> {
    let bytes = std::fs::read(path).with_context(|| format!("画像ファイルが見つかりません: {}", path.display()))?;
    let (_, mime, _) = detect(&bytes)?;
    let (img, orientation) = decode_oriented(&bytes)?;
    let small_enough = img.width().max(img.height()) <= CLAUDE_EDGE && bytes.len() <= CLAUDE_MAX_BYTES;
    if small_enough && orientation == Orientation::NoTransforms {
        return Ok((mime.to_string(), STANDARD.encode(&bytes)));
    }
    let resized = fit_within(&img, CLAUDE_EDGE);
    let (mut out, mut out_mime, _) = encode(&resized, 88)?;
    if out.len() > CLAUDE_MAX_BYTES {
        // 透過 PNG が大きすぎるときは JPEG にする
        out.clear();
        JpegEncoder::new_with_quality(&mut out, 85).encode_image(&resized.to_rgb8())?;
        out_mime = "image/jpeg";
    }
    Ok((out_mime.to_string(), STANDARD.encode(&out)))
}

/// ファイル名から拡張子を除いたもの（タイトルに使う）
pub fn title_from_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let stem = match base.rfind('.') {
        Some(i) if i > 0 => &base[..i],
        _ => base,
    };
    stem.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage, Rgba, RgbaImage};

    fn jpeg(w: u32, h: u32) -> Vec<u8> {
        let img = RgbImage::from_fn(w, h, |x, y| Rgb([(x % 256) as u8, (y % 256) as u8, 128]));
        let mut buf = Vec::new();
        JpegEncoder::new_with_quality(&mut buf, 80).encode_image(&img).unwrap();
        buf
    }

    fn png_alpha(w: u32, h: u32) -> Vec<u8> {
        let img = RgbaImage::from_fn(w, h, |x, _| Rgba([255, 0, 0, (x % 256) as u8]));
        let mut buf = Vec::new();
        DynamicImage::ImageRgba8(img)
            .write_to(&mut Cursor::new(&mut buf), ImageFormat::Png)
            .unwrap();
        buf
    }

    #[test]
    fn stores_original_and_thumbnail() {
        let dir = tempfile::tempdir().unwrap();
        let bytes = jpeg(2000, 1000);
        let s = store(&bytes, dir.path()).unwrap();
        assert_eq!((s.width, s.height, s.mime), (2000, 1000, "image/jpeg"));
        assert_eq!(
            std::fs::read(&s.file_path).unwrap(),
            bytes,
            "元のファイルはそのまま保存"
        );
        let thumb = image::open(&s.thumb_path).unwrap();
        assert_eq!(thumb.width().max(thumb.height()), THUMB_EDGE);
        // 同じ画像は同じハッシュ
        assert_eq!(store(&bytes, dir.path()).unwrap().hash, s.hash);
    }

    #[test]
    fn keeps_transparency_as_png() {
        let dir = tempfile::tempdir().unwrap();
        let s = store(&png_alpha(100, 50), dir.path()).unwrap();
        assert_eq!(s.mime, "image/png");
        assert!(s.thumb_path.to_string_lossy().ends_with(".png"));
    }

    #[test]
    fn rejects_unsupported_formats() {
        let dir = tempfile::tempdir().unwrap();
        let err = store(b"not an image at all", dir.path()).unwrap_err().to_string();
        assert!(err.contains("JPEG・PNG・GIF・WebP"));
    }

    #[test]
    fn prepares_images_for_claude() {
        let dir = tempfile::tempdir().unwrap();
        let small = dir.path().join("small.jpg");
        std::fs::write(&small, jpeg(800, 600)).unwrap();
        let (mime, b64) = for_claude(&small).unwrap();
        assert_eq!(mime, "image/jpeg");
        assert_eq!(
            STANDARD.decode(b64).unwrap(),
            std::fs::read(&small).unwrap(),
            "小さい画像はそのまま送る"
        );

        let big = dir.path().join("big.jpg");
        std::fs::write(&big, jpeg(2400, 1800)).unwrap();
        let (_, b64) = for_claude(&big).unwrap();
        let sent = image::load_from_memory(&STANDARD.decode(b64).unwrap()).unwrap();
        assert_eq!(sent.width().max(sent.height()), CLAUDE_EDGE, "大きい画像は縮小して送る");
    }

    #[test]
    fn title_from_file_name() {
        assert_eq!(
            title_from_name("スクリーンショット 2026-09-29.png"),
            "スクリーンショット 2026-09-29"
        );
        assert_eq!(title_from_name("C:\\\\Users\\\\a\\\\photo.jpeg"), "photo");
        assert_eq!(title_from_name(".hidden"), ".hidden");
    }
}
