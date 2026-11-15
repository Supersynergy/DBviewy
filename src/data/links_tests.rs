use super::*;

#[test]
fn plain_https_url() {
    assert_eq!(
        detect_url("https://example.com/path?q=1"),
        Some("https://example.com/path?q=1")
    );
}

#[test]
fn http_and_surrounding_whitespace_trimmed() {
    assert_eq!(detect_url("  http://a.b  "), Some("http://a.b"));
}

#[test]
fn scheme_is_case_insensitive() {
    assert_eq!(
        detect_url("HTTPS://Example.COM"),
        Some("HTTPS://Example.COM")
    );
}

#[test]
fn not_a_link() {
    assert_eq!(detect_url("see https://a.b for more"), None); // interior space
    assert_eq!(detect_url("ftp://a.b"), None); // wrong scheme
    assert_eq!(detect_url("example.com"), None); // no scheme
    assert_eq!(detect_url("https://"), None); // nothing after scheme
    assert_eq!(detect_url(""), None);
    assert_eq!(detect_url("hello world"), None);
}

#[test]
fn image_url_detected() {
    assert_eq!(
        detect_image_url("https://cdn.example.com/pics/item_1.webp"),
        Some("https://cdn.example.com/pics/item_1.webp")
    );
    assert_eq!(
        detect_image_url("  http://a.b/PICTURE.JPG?size=large  "),
        Some("http://a.b/PICTURE.JPG?size=large")
    );
    assert_eq!(
        detect_image_url("https://a.b/img.png#frag"),
        Some("https://a.b/img.png#frag")
    );
}

#[test]
fn non_image_url_rejected() {
    assert_eq!(detect_image_url("https://example.com/page.html"), None);
    assert_eq!(
        detect_image_url("https://example.com/download.cgi?id=4"),
        None
    );
    assert_eq!(detect_image_url("https://a.b/no-extension"), None);
    assert_eq!(detect_image_url("not a url.webp"), None);
    // Extension must be on the file name, not a path segment.
    assert_eq!(detect_image_url("https://a.b/images.v2/pic"), None);
    // bmp/avif are not decodable by the bundled image features.
    assert_eq!(detect_image_url("https://a.b/pic.bmp"), None);
    assert_eq!(detect_image_url("https://a.b/pic.avif"), None);
}

#[test]
fn image_cells_walks_rows_and_columns() {
    use crate::data::{CellValue, ColumnInfo, DataTable};
    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: "id".into(),
            data_type: "Int64".into(),
        },
        ColumnInfo {
            name: "pic".into(),
            data_type: "Utf8".into(),
        },
    ];
    let s = |v: &str| CellValue::String(v.to_string());
    t.rows = vec![
        vec![CellValue::Int(1), s("https://a.b/one.webp")],
        vec![CellValue::Int(2), s("https://a.b/two.webp")],
        vec![CellValue::Int(3), s("https://a.b/page.html")],
    ];
    assert_eq!(image_cells(&t, &[0, 1, 2]), vec![(0, 1), (1, 1)]);
    // Filtered order is honoured: reversed rows walk reversed.
    assert_eq!(image_cells(&t, &[1, 0]), vec![(1, 1), (0, 1)]);
}

#[test]
fn lead_domains_open_with_https_and_keep_paths() {
    for (text, col, expected) in [
        (" shop.de ", "Domain", Some("https://shop.de")),
        (
            "müller.de/impressum?a=1#info",
            "Website",
            Some("https://müller.de/impressum?a=1#info"),
        ),
        (
            "www.example.org/",
            "Quelle",
            Some("https://www.example.org/"),
        ),
        ("report.xlsx", "Datei", None),
        ("someone@example.org", "Domain", None),
        ("host.de\\@other.de", "Domain", None),
        ("a..de", "Domain", None),
        ("javascript:foo.de", "Domain", None),
    ] {
        assert_eq!(cell_url(text, col).as_deref(), expected);
    }
}

#[test]
fn pictures_accept_explicit_local_and_query_references() {
    for v in [
        "/home/user/My photo.png",
        "./files/photo.jpeg",
        "photo.jpg",
        "file:///tmp/My%20Photo.webp",
        "file://localhost/tmp/name.png",
        "https://cdn.example.test/download?filename=My%20Photo.png",
        "https://cdn.example.test/image?fm=webp",
    ] {
        assert_eq!(detect_image_url(v), Some(v), "{v}");
    }
    for v in [
        "file://server/share/x.png",
        "file:////server/share.png",
        "https://looks-like-image.png",
        "ftp://test/photo.jpg",
    ] {
        assert_eq!(detect_image_url(v), None, "{v}");
    }
}
