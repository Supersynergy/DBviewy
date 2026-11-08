use super::*;

const SERP: &str = r##"
<html><body><ol id="b_results">
<li class="b_algo"><h2><a target="_blank" href="https://www.bing.com/ck/a?!&amp;p=x&amp;u=a1aHR0cHM6Ly93d3cuZXhhbXBsZS5jb20vbG90LzMx&amp;ntb=1" h="ID=SERP,1">Lot 31 &ndash; Werkzeugset</a></h2><div class="b_caption"><p class="b_lineclamp2">Auktion: <strong>Werkzeug</strong>-Set, 40 Teile.</p></div></li>
<li class="b_algo"><h2><a href="https://direct.example.com/listing/7">Direct hit no redirect</a></h2><p>Plain second result.</p></li>
<li class="b_algo"><h2><a href="/relative/path">Not a real result</a></h2><p>skipped</p></li>
</ol></body></html>
"##;

#[test]
fn parses_hits_and_unwraps_bing_redirects() {
    let hits = parse_results(SERP, 10);
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].url, "https://www.example.com/lot/31");
    assert_eq!(hits[0].title, "Lot 31 – Werkzeugset");
    assert!(hits[0].snippet.contains("Werkzeug-Set"));
    assert_eq!(hits[1].url, "https://direct.example.com/listing/7");
}

#[test]
fn non_http_links_are_skipped() {
    assert!(
        !parse_results(SERP, 10)
            .iter()
            .any(|h| h.url.contains("relative"))
    );
}

#[test]
fn malformed_redirect_falls_back_to_raw_url() {
    let raw = "https://www.bing.com/ck/a?!&p=x&u=a1!!!not-base64";
    assert_eq!(unwrap_bing_redirect(raw), raw);
}

#[test]
fn urlencoding_escapes_utf8_and_reserved() {
    assert_eq!(urlencoded("a b&c"), "a%20b%26c");
    assert!(urlencoded("ü").starts_with('%'));
}
