//! Detect web links in cell text so the table view can render them as
//! hyperlinks and open them on Ctrl+click.
//!
//! Deliberately strict: a cell counts as a link only when its whole (trimmed)
//! text is a single `http://` or `https://` URL with no interior whitespace.
//! This keeps false positives out of ordinary prose cells that merely mention a
//! URL, and keeps the check cheap on the hot render path.

/// Return the URL when `text` is, after trimming, a single `http`/`https` link;
/// otherwise `None`.
pub fn detect_url(text: &str) -> Option<&str> {
    let trimmed = text.trim();
    if trimmed.len() < "http://".len() {
        return None;
    }
    let lower_ok = trimmed
        .get(..8)
        .map(|p| {
            let p = p.to_ascii_lowercase();
            p.starts_with("http://") || p.starts_with("https://")
        })
        .unwrap_or(false);
    if !lower_ok {
        return None;
    }
    // A single token: no ASCII whitespace anywhere.
    if trimmed.chars().any(|c| c.is_whitespace()) {
        return None;
    }
    // Require at least one char after the scheme separator.
    let after_scheme = trimmed
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or("");
    if after_scheme.is_empty() {
        return None;
    }
    Some(trimmed)
}

/// Recognise addresses without a scheme in website fields. Ordinary dotted
/// values in arbitrary columns (filenames, versions) stay ordinary text.
pub fn cell_url(text: &str, column: &str) -> Option<String> {
    if let Some(url) = detect_url(text) {
        return Some(url.to_owned());
    }
    let address = text.trim();
    let host = address.split(['/', '?', '#']).next()?;
    if address
        .chars()
        .any(|c| c.is_whitespace() || c.is_control() || c == '\\')
    {
        return None;
    }
    if !host.contains('.')
        || !(["domain", "website", "webseite", "url", "homepage"]
            .iter()
            .any(|name| column.trim().eq_ignore_ascii_case(name))
            || host
                .get(..4)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("www.")))
    {
        return None;
    }
    let suffix = host.rsplit('.').next()?;
    if host.len() > 253
        || suffix.len() < 2
        || !suffix.chars().all(char::is_alphabetic)
        || !host.split('.').all(|part| {
            !part.is_empty()
                && part.len() <= 63
                && !part.starts_with('-')
                && !part.ends_with('-')
                && part.chars().all(|c| c.is_alphanumeric() || c == '-')
        })
    {
        return None;
    }
    Some(format!("https://{address}"))
}

/// File extensions `detect_image_url` treats as pictures. Kept to what
/// `binary_kind::decode_image` can actually render: the `image` crate's
/// enabled formats plus SVG, which that function draws through its own path.
const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "svg"];

fn image_extension(name: &str) -> bool {
    name.rsplit(['/', '\\'])
        .next()
        .and_then(|name| name.rsplit_once('.'))
        .is_some_and(|(_, ext)| {
            IMAGE_EXTENSIONS
                .iter()
                .any(|known| ext.eq_ignore_ascii_case(known))
        })
}

/// Accept picture references, file names and local file URIs. Detection never
/// opens a file. The image cache checks the user's file grant before access.
pub fn detect_image_url(text: &str) -> Option<&str> {
    let text = text.trim();
    if text.is_empty() || text.len() > 8192 || text.chars().any(char::is_control) {
        return None;
    }
    if crate::data::embedded_media::is_reference(text) {
        return Some(text);
    }
    if let Some(url) = detect_url(text) {
        let rest = url.split_once("://")?.1.split('#').next()?;
        let (address, query) = rest.split_once('?').unwrap_or((rest, ""));
        let path = address.split_once('/').map_or("", |(_, path)| path);
        let query_image = query
            .split('&')
            .filter_map(|part| part.split_once('='))
            .any(|(name, value)| {
                if matches!(name, "format" | "fm") {
                    return IMAGE_EXTENSIONS
                        .iter()
                        .any(|ext| value.eq_ignore_ascii_case(ext));
                }
                let value = percent_encoding::percent_decode_str(value).decode_utf8_lossy();
                image_extension(&value)
            });
        return (image_extension(path) || query_image).then_some(text);
    }
    if text
        .get(..7)
        .is_some_and(|p| p.eq_ignore_ascii_case("file://"))
    {
        let path = local_file_uri(text)?;
        return image_extension(&path).then_some(text);
    }
    if text.contains("://") || text.starts_with("//") || text.starts_with(r"\\") {
        return None;
    }
    let looks_like_path = text.contains(['/', '\\']) || !text.chars().any(char::is_whitespace);
    (looks_like_path && image_extension(text)).then_some(text)
}

/// File URIs with a remote authority are never local image sources.
pub(crate) fn local_file_uri(text: &str) -> Option<std::borrow::Cow<'_, str>> {
    let rest = text.get(7..)?;
    let (host, _) = rest.split_once('/')?;
    if !host.is_empty() && !host.eq_ignore_ascii_case("localhost") {
        return None;
    }
    let local = rest.get(host.len()..)?.split(['?', '#']).next()?;
    let decoded = percent_encoding::percent_decode_str(local)
        .decode_utf8()
        .ok()?;
    if decoded.contains(['\0', '\\']) || decoded.starts_with("//") {
        return None;
    }
    #[cfg(windows)]
    if decoded.as_bytes().get(2) == Some(&b':') {
        return Some(std::borrow::Cow::Owned(decoded[1..].to_owned()));
    }
    Some(decoded)
}

/// Every `(row, col)` cell whose text is an image URL, in `order` row order
/// (the filtered view) and column order. `order` holds actual table row
/// indices; pass `(0..table.row_count()).collect()` for the unfiltered table.
/// Used to build the image viewer's previous/next walk.
pub fn image_cells(table: &crate::data::DataTable, order: &[usize]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    'rows: for &row in order {
        for col in 0..table.col_count() {
            if let Some(v) = table.get(row, col)
                && detect_image_url(&v.to_string()).is_some()
            {
                out.push((row, col));
                if out.len() >= MAX_IMAGE_CELLS {
                    break 'rows;
                }
            }
        }
    }
    out
}

/// A table with more image cells than this gets a truncated walk: the viewer
/// still opens and browses, it just stops offering "next" past the cap. The
/// cap exists so opening the viewer on a pathological sheet cannot allocate
/// an unbounded list.
const MAX_IMAGE_CELLS: usize = 100_000;

#[cfg(test)]
#[path = "links_tests.rs"]
mod tests;
