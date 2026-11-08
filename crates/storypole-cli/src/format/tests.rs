//! `show` prints the description (`GB-312`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::format_description;

#[test]
fn the_description_is_rendered_with_its_paragraphs_intact() {
    let rendered = format_description("First paragraph.\n\nSecond paragraph.");

    assert_eq!(
        rendered, "Description:\n  First paragraph.\n\n  Second paragraph.\n",
        "paragraph breaks survive, and every line is indented under the label"
    );
}

#[test]
fn an_empty_description_renders_nothing_at_all() {
    // A blank label would be noise on every ticket that has no description, which is most of them.
    assert_eq!(format_description(""), "");
    assert_eq!(format_description("   \n\n "), "");
}

#[test]
fn a_single_line_description_keeps_its_wrapping_intact() {
    let long_line = "x".repeat(200);
    let rendered = format_description(&long_line);
    assert!(
        rendered.contains(&long_line),
        "the renderer does not re-wrap prose the author already wrapped"
    );
}
