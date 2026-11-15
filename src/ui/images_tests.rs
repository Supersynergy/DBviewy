use super::*;

#[test]
fn wide_picture_shrinks_to_the_limit() {
    assert_eq!(fit_to_side(16000, 4000, 8192), (8192, 2048));
    assert_eq!(fit_to_side(4000, 16000, 8192), (2048, 8192));
}

#[test]
fn small_picture_is_untouched() {
    assert_eq!(fit_to_side(800, 600, 8192), (800, 600));
    assert_eq!(fit_to_side(8192, 8192, 8192), (8192, 8192));
}

#[test]
fn thin_picture_keeps_at_least_one_pixel() {
    assert_eq!(fit_to_side(16384, 1, 8192), (8192, 1));
}

fn key(source: &str, preview: bool) -> Key {
    Key {
        source: source.to_owned(),
        generation: 0,
        preview,
    }
}

#[test]
fn eviction_drops_oldest_first() {
    let mut inner = Inner::default();
    for i in 0..(MAX_ENTRIES + 10) {
        insert(
            &mut inner,
            key(&format!("u{i}"), false),
            Entry::Failed("x".into(), Instant::now()),
        );
    }
    assert_eq!(inner.entries.len(), MAX_ENTRIES);
    assert!(!inner.entries.contains_key(&key("u9", false)));
    assert!(
        inner
            .entries
            .contains_key(&key(&format!("u{}", MAX_ENTRIES + 9), false))
    );
    assert_eq!(inner.order[0].len(), inner.entries.len());
}

#[test]
fn refresh_of_known_url_does_not_duplicate_lru() {
    let mut inner = Inner::default();
    for reason in ["a", "b"] {
        insert(
            &mut inner,
            key("u", false),
            Entry::Failed(reason.into(), Instant::now()),
        );
    }
    assert_eq!(inner.order[0].len(), 1);
    assert!(matches!(inner.entries[&key("u", false)], Entry::Failed(ref e, _) if e == "b"));
}

#[test]
fn selected_directory_limits_local_media_and_preview_preserves_detail() {
    let dir = tempfile::tempdir().unwrap();
    let allowed = dir.path().join("album");
    std::fs::create_dir(&allowed).unwrap();
    let image = image::RgbaImage::new(400, 200);
    image.save(allowed.join("photo.png")).unwrap();
    image.save(dir.path().join("private.png")).unwrap();
    let scope = Scope {
        directory: Some(allowed.canonicalize().unwrap()),
        ..Scope::default()
    };
    let agent = ureq::Agent::new_with_defaults();
    assert!(source::load(&key("photo.png", false), &Scope::default(), &agent, None).is_err());
    let thumb = source::load(&key("photo.png", false), &scope, &agent, None).unwrap();
    let preview = source::load(&key("photo.png", true), &scope, &agent, None).unwrap();
    assert_eq!(thumb.dimensions(), (160, 80));
    assert_eq!(preview.dimensions(), (400, 200));
    assert!(source::load(&key("../private.png", true), &scope, &agent, None).is_err());
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(dir.path().join("private.png"), allowed.join("alias.png"))
            .unwrap();
        assert!(source::load(&key("alias.png", true), &scope, &agent, None).is_err());
    }
}

#[test]
fn remote_thumbnail_survives_a_new_cache_without_network() {
    use std::io::{BufRead, BufReader, Cursor, Write};
    use std::net::TcpListener;
    let mut png = Cursor::new(Vec::new());
    image::RgbaImage::new(500, 250)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let bytes = png.into_inner();
    let socket = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/photo.png?w=500", socket.local_addr().unwrap());
    let serve = std::thread::spawn(move || {
        let (mut stream, _) = socket.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(4)))
            .unwrap();
        let mut reader = BufReader::new(&stream);
        let mut line = String::new();
        while reader.read_line(&mut line).unwrap() > 0 && line != "\r\n" {
            line.clear();
        }
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            bytes.len()
        )
        .unwrap();
        stream.write_all(&bytes).unwrap();
    });
    let agent = ureq::Agent::config_builder().proxy(None).build().into();
    let dir = tempfile::tempdir().unwrap();
    let scope = Scope::default();
    for _ in 0..2 {
        let cache = source::DiskCache::new(dir.path().to_owned());
        assert_eq!(
            source::load(&key(&url, false), &scope, &agent, Some(&cache))
                .unwrap()
                .dimensions(),
            (160, 80)
        );
    }
    serve.join().unwrap();
    let file = std::fs::read_dir(dir.path())
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    assert_eq!(file.extension().unwrap(), "webp");
    assert!(std::fs::metadata(file).unwrap().len() < 4096);
}

#[test]
fn switching_databases_isolates_embedded_thumbnails_and_local_grants() {
    use std::io::Cursor;
    let reference = format!("dbviewy-media:{}", "a".repeat(64));
    let dir = tempfile::tempdir().unwrap();
    let cache = ImageCache::new();
    let agent = ureq::Agent::new_with_defaults();
    let mut previous = None;
    for (name, width) in [("one.sqlite", 17), ("two.sqlite", 29)] {
        let path = dir.path().join(name);
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch("CREATE TABLE dbviewy_media(id TEXT, thumbnail BLOB, preview BLOB)")
            .unwrap();
        let mut bytes = Cursor::new(Vec::new());
        image::RgbaImage::new(width, 1)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        conn.execute(
            "INSERT INTO dbviewy_media VALUES (?1, ?2, ?2)",
            rusqlite::params![
                reference.strip_prefix("dbviewy-media:").unwrap(),
                bytes.into_inner()
            ],
        )
        .unwrap();
        cache.set_document(Some(&path), 101);
        let key = cache.key(&reference, false);
        if let Some(old) = previous {
            assert!(key != old);
            assert!(cache.scope.borrow().directory.is_none());
        }
        let pixels = source::load(&key, &cache.scope.borrow(), &agent, None).unwrap();
        assert_eq!(pixels.width(), width);
        cache.allow_local_directory(dir.path()).unwrap();
        previous = Some(key);
    }
}

#[test]
fn two_unsaved_tiles_have_distinct_local_grants_and_stable_keys() {
    let dir = tempfile::tempdir().unwrap();
    let cache = ImageCache::new();
    cache.set_document(None, 1);
    cache.allow_local_directory(dir.path()).unwrap();
    let first = cache.key("picture.png", false);
    cache.set_document(None, 2);
    assert!(cache.scope.borrow().directory.is_none());
    assert!(cache.key("picture.png", false) != first);
    cache.set_document(None, 1);
    assert!(cache.scope.borrow().directory.is_some());
    assert!(cache.key("picture.png", false) == first);
}
