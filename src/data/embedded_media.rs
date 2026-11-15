//! Portable SQLite image attachments. The grid holds short references so opening a
//! product table never materialises an entire catalogue of full-size pictures.
use super::transform::hash_columns::HashColumnsAlgo;
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OpenFlags, params};
use std::io::{Cursor, Read};
use std::path::Path;
use std::time::Duration;

const PREFIX: &str = "dbviewy-media:";
const LIMIT: u64 = 32 * 1024 * 1024;
const STORE: &str = "CREATE TABLE IF NOT EXISTS dbviewy_media (
    id TEXT PRIMARY KEY, preview BLOB NOT NULL, thumbnail BLOB NOT NULL)";
pub const ALBUM: &str = "Bildimporte";

pub fn is_reference(value: &str) -> bool {
    value.strip_prefix(PREFIX).is_some_and(|id| {
        id.len() == 64
            && id
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

pub fn read_picture(database: &Path, reference: &str, thumbnail: bool) -> Result<Vec<u8>, String> {
    fn read(database: &Path, reference: &str, thumbnail: bool) -> Result<Vec<u8>> {
        ensure!(is_reference(reference), "Invalid image reference");
        let conn = Connection::open_with_flags(database, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        conn.pragma_update(None, "trusted_schema", false)?;
        let column = if thumbnail { "thumbnail" } else { "preview" };
        let sql = format!(
            "SELECT {column} FROM dbviewy_media WHERE id=?1 AND length({column}) BETWEEN 1 AND ?2"
        );
        Ok(conn.query_row(
            &sql,
            params![&reference[PREFIX.len()..], LIMIT as i64],
            |r| r.get(0),
        )?)
    }
    read(database, reference, thumbnail).map_err(|e| format!("{e:#}"))
}

pub enum Source {
    File(std::path::PathBuf),
    Url(String),
}

impl Source {
    fn bytes(&self) -> Result<Vec<u8>> {
        let bytes = match self {
            Self::File(path) => {
                let file = std::fs::File::open(path)?;
                ensure!(file.metadata()?.is_file(), "Select an image file");
                let mut bytes = Vec::new();
                file.take(LIMIT + 1).read_to_end(&mut bytes)?;
                bytes
            }
            Self::Url(url) => {
                ensure!(
                    super::links::detect_url(url).is_some(),
                    "Enter an HTTP or HTTPS image address"
                );
                let agent: ureq::Agent = ureq::Agent::config_builder()
                    .timeout_global(Some(Duration::from_secs(20)))
                    .build()
                    .into();
                let mut response = agent.get(url.trim()).call()?;
                ensure!(response.status().is_success(), "HTTP {}", response.status());
                response
                    .body_mut()
                    .with_config()
                    .limit(LIMIT)
                    .read_to_vec()?
            }
        };
        ensure!(bytes.len() as u64 <= LIMIT, "Image exceeds 32 MiB");
        Ok(bytes)
    }
}

/// Only the user's explicit import action writes an album entry. Both sizes and
/// provenance commit together, including on repeat imports of the same address.
pub fn import(database: &Path, source: Source) -> Result<String> {
    let bytes = source.bytes()?;
    let label = match source {
        Source::File(path) => path
            .file_name()
            .context("Missing file name")?
            .to_string_lossy()
            .into_owned(),
        Source::Url(url) => url.trim().to_owned(),
    };
    let pixels = super::binary_kind::decode_image(&bytes)?;
    let picture = image::DynamicImage::ImageRgba8(pixels);
    let thumb = picture.thumbnail(256, 256);
    let mut thumbnail = Cursor::new(Vec::new());
    if thumb.to_rgba8().pixels().any(|p| p[3] != 255) {
        thumb.write_to(&mut thumbnail, image::ImageFormat::WebP)?;
    } else {
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut thumbnail, 78)
            .encode_image(&image::DynamicImage::ImageRgb8(thumb.to_rgb8()))?;
    }
    let id = HashColumnsAlgo::Sha256.hex(&bytes);
    let reference = format!("{PREFIX}{id}");
    let mut conn = Connection::open_with_flags(database, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    conn.pragma_update(None, "trusted_schema", false)?;
    conn.busy_timeout(Duration::from_secs(3))?;
    let tx = conn.transaction()?;
    tx.execute_batch(STORE)?;
    tx.execute_batch("CREATE TABLE IF NOT EXISTS Bildimporte (Bild TEXT NOT NULL, Quelle TEXT NOT NULL, UNIQUE(Bild, Quelle))")?;
    tx.execute("INSERT INTO dbviewy_media(id,preview,thumbnail) VALUES (?1,?2,?3) ON CONFLICT(id) DO NOTHING",
               params![id,bytes,thumbnail.into_inner()])?;
    tx.execute(
        "INSERT INTO Bildimporte(Bild,Quelle) VALUES (?1,?2) ON CONFLICT(Bild,Quelle) DO NOTHING",
        params![reference, label],
    )?;
    tx.commit()?;
    Ok(reference)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn embedded_media_import_round_trip_and_database_isolation() {
        let tmp = tempfile::tempdir().unwrap();
        let db = tmp.path().join("album.sqlite");
        Connection::open(&db).unwrap();
        let image = tmp.path().join("sample.png");
        image::RgbImage::new(700, 350).save(&image).unwrap();
        let reference = import(&db, Source::File(image.clone())).unwrap();
        assert_eq!(import(&db, Source::File(image.clone())).unwrap(), reference);
        let conn = Connection::open(&db).unwrap();
        assert_eq!(
            conn.query_row("select count(*) from Bildimporte", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            read_picture(&db, &reference, false).unwrap(),
            std::fs::read(image).unwrap()
        );
        let thumb = image::load_from_memory(&read_picture(&db, &reference, true).unwrap()).unwrap();
        assert_eq!((thumb.width(), thumb.height()), (256, 128));
        let other = tmp.path().join("other.sqlite");
        Connection::open(&other).unwrap();
        assert!(read_picture(&other, &reference, false).is_err());
        assert!(!is_reference("dbviewy-media:bad"));
    }
}
