//! Syntax highlighting for fenced code blocks.
//!
//! Wraps [`syntect`] to turn a code string + language hint into per-line,
//! ratatui-styled segments. The heavyweight syntax and theme definition sets
//! are loaded once and cached for the process lifetime, so highlighting a code
//! block is cheap after the first call. The chat view caches rendered lines per
//! message, so a given block is only highlighted when it (or the window width)
//! changes.

use std::io::Cursor;
use std::sync::OnceLock;

use ratatui::style::{Color, Modifier, Style};
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, Style as SynStyle, Theme, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

/// One highlighted segment: the styled text and the style to render it with.
pub type StyledSegment = (Style, String);

/// Vibrant Monokai theme embedded for dark application themes. Monokai colors
/// many more scopes (functions, types, numbers, strings) than the bundled
/// base16 palettes, giving fuller-looking highlighting.
const MONOKAI_THEME: &str = include_str!("../assets/Monokai.tmTheme");
/// Fallback bundled theme name if the embedded dark theme fails to load.
const DARK_FALLBACK: &str = "base16-ocean.dark";
/// Bundled syntect theme used for light application themes.
const LIGHT_THEME: &str = "InspiredGitHub";

fn syntax_set() -> &'static SyntaxSet {
    static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
    SYNTAX_SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn theme_set() -> &'static ThemeSet {
    static THEME_SET: OnceLock<ThemeSet> = OnceLock::new();
    THEME_SET.get_or_init(ThemeSet::load_defaults)
}

/// The color theme used for highlighting, cached per light/dark mode.
fn highlight_theme(dark: bool) -> &'static Theme {
    static DARK: OnceLock<Theme> = OnceLock::new();
    static LIGHT: OnceLock<Theme> = OnceLock::new();
    if dark {
        DARK.get_or_init(|| {
            ThemeSet::load_from_reader(&mut Cursor::new(MONOKAI_THEME.as_bytes()))
                .unwrap_or_else(|_| theme_set().themes[DARK_FALLBACK].clone())
        })
    } else {
        LIGHT.get_or_init(|| theme_set().themes[LIGHT_THEME].clone())
    }
}

/// Whether a language hint can be resolved to a real syntax definition.
///
/// Used to decide if a code block should be highlighted or rendered plainly.
pub fn is_language_supported(language: &str) -> bool {
    let ss = syntax_set();
    ss.find_syntax_by_token(language).is_some()
}

/// Highlight `code` and return one `Vec<StyledSegment>` per line.
///
/// The language hint is resolved against syntect's token names/extensions
/// (e.g. "rust", "rs", "python", "js"). If it cannot be resolved, the first
/// line is sniffed; failing that, the code is returned as a single unstyled
/// segment per line (so callers can still add line numbers and a background).
///
/// `dark` selects between bundled light/dark color themes.
pub fn highlight(code: &str, language: Option<&str>, dark: bool) -> Vec<Vec<StyledSegment>> {
    let ss = syntax_set();

    // Resolve the syntax: by language token, then by first-line sniffing.
    let syntax = language
        .and_then(|lang| ss.find_syntax_by_token(lang))
        .or_else(|| ss.find_syntax_by_first_line(code.lines().next().unwrap_or("")))
        .unwrap_or_else(|| ss.find_syntax_plain_text());

    let theme = highlight_theme(dark);
    let mut highlighter = HighlightLines::new(syntax, theme);
    let mut out: Vec<Vec<StyledSegment>> = Vec::new();

    for line in LinesWithEndings::from(code) {
        match highlighter.highlight_line(line, ss) {
            Ok(ranges) => {
                let segments = ranges
                    .into_iter()
                    .map(|(syn_style, text)| {
                        // Drop the trailing newline; lines are joined by the
                        // renderer, not by embedded newlines.
                        let text = text.strip_suffix('\n').unwrap_or(text);
                        (to_ratatui_style(syn_style), text.to_string())
                    })
                    .filter(|(_, text)| !text.is_empty())
                    .collect();
                out.push(segments);
            }
            // On any highlighter error, fall back to the raw line unstyled so
            // no content is ever lost.
            Err(_) => {
                let text = line.strip_suffix('\n').unwrap_or(line);
                out.push(vec![(Style::default(), text.to_string())]);
            }
        }
    }

    // `LinesWithEndings` yields nothing for empty input; represent that as a
    // single empty line so the block still renders.
    if out.is_empty() {
        out.push(vec![(Style::default(), String::new())]);
    }

    out
}

/// Convert a syntect foreground style into a ratatui style (RGB + font style).
fn to_ratatui_style(syn: SynStyle) -> Style {
    let fg = syn.foreground;
    let mut style = Style::default().fg(Color::Rgb(fg.r, fg.g, fg.b));

    let fs = syn.font_style;
    if fs.contains(FontStyle::BOLD) {
        style = style.add_modifier(Modifier::BOLD);
    }
    if fs.contains(FontStyle::ITALIC) {
        style = style.add_modifier(Modifier::ITALIC);
    }
    if fs.contains(FontStyle::UNDERLINE) {
        style = style.add_modifier(Modifier::UNDERLINED);
    }

    style
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlights_rust_into_multiple_segments() {
        let code = "fn main() {\n    let x = 42;\n}";
        let lines = highlight(code, Some("rust"), true);
        assert_eq!(lines.len(), 3, "one entry per source line");
        // A highlighted keyword line should yield more than one colored segment.
        assert!(
            lines[0].len() > 1,
            "expected multiple highlighted segments on the fn line"
        );
        // Reassembling the segments must reproduce the original line exactly.
        let reassembled: String = lines[1].iter().map(|(_, t)| t.as_str()).collect();
        assert_eq!(reassembled, "    let x = 42;");
    }

    #[test]
    fn unknown_language_falls_back_without_losing_content() {
        let code = "some::arbitrary text 123";
        let lines = highlight(code, Some("not-a-real-language"), true);
        assert_eq!(lines.len(), 1);
        let reassembled: String = lines[0].iter().map(|(_, t)| t.as_str()).collect();
        assert_eq!(reassembled, code);
    }

    #[test]
    fn empty_code_yields_one_empty_line() {
        let lines = highlight("", Some("rust"), true);
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn language_support_detection() {
        assert!(is_language_supported("rust"));
        assert!(is_language_supported("python"));
        assert!(!is_language_supported("definitely-not-a-language"));
    }
}
