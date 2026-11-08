use super::*;

/// Script and style content must not leak into the text, tags become line
/// breaks, entities decode.
#[test]
fn html_to_text_extracts_readable_text() {
    let html = r#"<html><head><style>body { color: red }</style></head>
        <body><h1>Lot 31</h1><p>Price: 1&#8202;234 &amp; tax</p>
        <script>alert('x')</script><ul><li>first</li><li>second</li></ul>
        </body></html>"#;
    let text = html_to_text(html);
    assert!(text.contains("Lot 31"), "missing heading: {text}");
    assert!(text.contains("Price: 1 234 & tax"), "entities: {text}");
    assert!(text.contains("first"));
    assert!(text.contains("second"));
    assert!(!text.contains("alert"), "script leaked: {text}");
    assert!(!text.contains("color"), "style leaked: {text}");
    assert!(!text.contains('<'), "tag leaked: {text}");
}

/// Comments, doctypes and unterminated tails don't break the scan.
#[test]
fn html_to_text_survives_malformed_input() {
    let text = html_to_text("<!doctype html><p>hi <!-- note --> there<p>second");
    assert!(text.contains("hi"));
    assert!(text.contains("there"));
    assert!(text.contains("second"));
}

#[test]
fn decode_entities_handles_named_numeric_and_unknown() {
    assert_eq!(decode_entities("a &amp; b &#65; &#x42;"), "a & b A B");
    assert_eq!(decode_entities("plain"), "plain");
    assert!(decode_entities("a &bogus; b").contains("&bogus;"));
}

#[test]
fn url_scheme_is_enforced() {
    let ctx = ToolContext::for_mcp(Some(1000), 65536, false, true, Vec::new(), false, 0);
    let p = Params {
        url: "file:///etc/passwd".to_string(),
        max_chars: None,
        raw: false,
    };
    assert!(run(&ctx, &p).is_err());
    let p = Params {
        url: "ftp://host/x".to_string(),
        ..p
    };
    assert!(run(&ctx, &p).is_err());
}
