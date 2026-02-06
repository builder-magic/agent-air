//! Markdown parsing utilities for TUI rendering
//!
//! Uses pulldown-cmark for CommonMark parsing.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

use super::table::{is_table_line, is_table_separator, render_table};
use super::themes::Theme;

// Prefixes for message formatting
const ASSISTANT_PREFIX: &str = "\u{25C6} "; // diamond
const CONTINUATION: &str = "  ";

/// Parse markdown text into styled ratatui spans
pub fn parse_to_spans(text: &str, theme: &Theme) -> Vec<Span<'static>> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);

    let parser = Parser::new_ext(text, options);
    let mut spans = Vec::new();
    let mut style_stack: Vec<Modifier> = Vec::new();
    let mut color_stack: Vec<Color> = Vec::new();
    let mut link_url_stack: Vec<String> = Vec::new();

    for event in parser {
        match event {
            // Inline formatting start
            Event::Start(Tag::Strong) => {
                style_stack.push(theme.bold());
            }
            Event::Start(Tag::Emphasis) => {
                style_stack.push(theme.italic());
            }
            Event::Start(Tag::Strikethrough) => {
                style_stack.push(theme.strikethrough());
            }

            // Inline formatting end
            Event::End(TagEnd::Strong)
            | Event::End(TagEnd::Emphasis)
            | Event::End(TagEnd::Strikethrough) => {
                style_stack.pop();
            }

            // Text content
            Event::Text(t) => {
                let style = build_style(&style_stack, &color_stack);
                spans.push(Span::styled(t.into_string(), style));
            }

            // Inline code gets special styling
            Event::Code(code) => {
                spans.push(Span::styled(code.into_string(), theme.inline_code()));
            }

            // Soft breaks become spaces
            Event::SoftBreak => {
                spans.push(Span::raw(" "));
            }

            // Hard breaks preserved
            Event::HardBreak => {
                spans.push(Span::raw("\n"));
            }

            // Skip block-level events - we handle text line by line
            Event::Start(Tag::Paragraph)
            | Event::End(TagEnd::Paragraph)
            | Event::Start(Tag::Heading { .. })
            | Event::End(TagEnd::Heading(_)) => {}

            // Links - render text in link color, then append URL
            Event::Start(Tag::Link { dest_url, .. }) => {
                // Extract color from theme link_text style
                if let Some(color) = theme.link_text().fg {
                    color_stack.push(color);
                }
                // Store URL to append after link text
                link_url_stack.push(dest_url.into_string());
            }
            Event::End(TagEnd::Link) => {
                color_stack.pop();
                // Append URL after link text
                if let Some(url) = link_url_stack.pop()
                    && !url.is_empty()
                {
                    spans.push(Span::styled(format!(" ({})", url), theme.link_url()));
                }
            }

            // Other events we don't handle yet
            _ => {}
        }
    }

    spans
}

/// Build a Style from the current modifier and color stacks
fn build_style(modifiers: &[Modifier], colors: &[Color]) -> Style {
    let mut style = Style::default();

    // Apply all modifiers
    for modifier in modifiers {
        style = style.add_modifier(*modifier);
    }

    // Apply the most recent color if any
    if let Some(&color) = colors.last() {
        style = style.fg(color);
    }

    style
}

/// Parse markdown and split into words with their styles
///
/// Useful for word-wrapping while preserving styles
pub fn parse_to_styled_words(text: &str, theme: &Theme) -> Vec<(String, Style)> {
    let spans = parse_to_spans(text, theme);
    let mut words = Vec::new();

    for span in spans {
        let content = span.content.to_string();
        let style = span.style;

        // Split each span's content into words
        for word in content.split_whitespace() {
            words.push((word.to_string(), style));
        }
    }

    words
}

/// Content segment type for splitting text and tables
pub enum ContentSegment {
    Text(String),
    Table(Vec<String>),
    /// Code block with optional language hint.
    CodeBlock {
        code: String,
        /// Language hint (e.g., "rust", "python") - parsed from markdown but
        /// not yet used. Reserved for future syntax highlighting support.
        #[allow(dead_code)]
        language: Option<String>,
    },
}

/// Wrap text with a first-line prefix and continuation prefix
///
/// Uses pulldown-cmark for inline styling (bold, italic, code)
pub fn wrap_with_prefix(
    text: &str,
    first_prefix: &str,
    first_prefix_style: Style,
    cont_prefix: &str,
    max_width: usize,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let text_width = max_width.saturating_sub(first_prefix.chars().count());

    if text_width == 0 || text.is_empty() {
        // Parse markdown even for short text
        let spans = parse_to_spans(text, theme);
        let mut result_spans = vec![Span::styled(first_prefix.to_string(), first_prefix_style)];
        result_spans.extend(spans);
        return vec![Line::from(result_spans)];
    }

    // Parse markdown into styled words for wrapping
    let styled_words = parse_to_styled_words(text, theme);

    // Word-wrap the styled words
    let mut current_line_spans: Vec<Span<'static>> = Vec::new();
    let mut current_line_len = 0usize;
    let mut is_first_line = true;

    for (word, style) in styled_words {
        let word_len = word.chars().count();
        let would_be_len = if current_line_len == 0 {
            word_len
        } else {
            current_line_len + 1 + word_len
        };

        if would_be_len > text_width && current_line_len > 0 {
            // Emit current line
            let prefix = if is_first_line {
                first_prefix
            } else {
                cont_prefix
            };
            let prefix_style = if is_first_line {
                first_prefix_style
            } else {
                Style::default()
            };
            let mut line_spans = vec![Span::styled(prefix.to_string(), prefix_style)];
            line_spans.append(&mut current_line_spans);
            lines.push(Line::from(line_spans));

            current_line_spans.push(Span::styled(word, style));
            current_line_len = word_len;
            is_first_line = false;
        } else {
            if current_line_len > 0 {
                current_line_spans.push(Span::raw(" "));
                current_line_len += 1;
            }
            current_line_spans.push(Span::styled(word, style));
            current_line_len += word_len;
        }
    }

    // Emit remaining text
    if !current_line_spans.is_empty() || is_first_line {
        let prefix = if is_first_line {
            first_prefix
        } else {
            cont_prefix
        };
        let prefix_style = if is_first_line {
            first_prefix_style
        } else {
            Style::default()
        };
        let mut line_spans = vec![Span::styled(prefix.to_string(), prefix_style)];
        line_spans.extend(current_line_spans);
        lines.push(Line::from(line_spans));
    }

    lines
}

/// Check if text starts with a heading marker using pulldown-cmark
///
/// This is more robust than string matching and handles edge cases correctly
pub fn detect_heading_level(text: &str) -> Option<u8> {
    let parser = Parser::new(text);
    for event in parser {
        if let Event::Start(Tag::Heading { level, .. }) = event {
            return Some(level as u8);
        }
    }
    None
}

/// Get style for heading level
pub fn heading_style(level: u8, theme: &Theme) -> Style {
    match level {
        1 => theme.heading_1(),
        2 => theme.heading_2(),
        3 => theme.heading_3(),
        _ => theme.heading_4(),
    }
}

/// Check if a line starts a fenced code block (``` or ~~~)
fn is_code_fence(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    if trimmed.starts_with("```") {
        Some(trimmed.strip_prefix("```").unwrap_or("").trim())
    } else if trimmed.starts_with("~~~") {
        Some(trimmed.strip_prefix("~~~").unwrap_or("").trim())
    } else {
        None
    }
}

/// Check if a line ends a fenced code block
fn is_code_fence_end(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed == "```" || trimmed == "~~~"
}

/// Split content into text, table, and code block segments
pub fn split_content_segments(content: &str) -> Vec<ContentSegment> {
    let lines: Vec<&str> = content.lines().collect();
    let mut segments = Vec::new();
    let mut current_text = String::new();
    let mut i = 0;

    while i < lines.len() {
        // Check for fenced code block
        if let Some(lang) = is_code_fence(lines[i]) {
            // Found a code block! First, save any accumulated text
            if !current_text.is_empty() {
                segments.push(ContentSegment::Text(current_text));
                current_text = String::new();
            }

            let language = if lang.is_empty() {
                None
            } else {
                Some(lang.to_string())
            };
            i += 1; // Skip the opening fence

            // Collect code block content until closing fence
            let mut code_content = String::new();
            while i < lines.len() && !is_code_fence_end(lines[i]) {
                if !code_content.is_empty() {
                    code_content.push('\n');
                }
                code_content.push_str(lines[i]);
                i += 1;
            }

            // Skip the closing fence if present
            if i < lines.len() && is_code_fence_end(lines[i]) {
                i += 1;
            }

            segments.push(ContentSegment::CodeBlock {
                code: code_content,
                language,
            });
        }
        // Check if this might be a table (line with | and next line is separator)
        else if is_table_line(lines[i]) && i + 1 < lines.len() && is_table_separator(lines[i + 1])
        {
            // Found a table! First, save any accumulated text
            if !current_text.is_empty() {
                segments.push(ContentSegment::Text(current_text));
                current_text = String::new();
            }

            // Collect all table lines
            let mut table_lines = Vec::new();
            while i < lines.len() && is_table_line(lines[i]) {
                table_lines.push(lines[i].to_string());
                i += 1;
            }
            segments.push(ContentSegment::Table(table_lines));
        } else {
            // Regular text line
            if !current_text.is_empty() {
                current_text.push('\n');
            }
            current_text.push_str(lines[i]);
            i += 1;
        }
    }

    // Don't forget remaining text
    if !current_text.is_empty() {
        segments.push(ContentSegment::Text(current_text));
    }

    segments
}

/// Render markdown content with diamond prefix and manual wrapping
pub fn render_markdown_with_prefix(
    content: &str,
    max_width: usize,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let segments = split_content_segments(content);

    let mut all_lines = Vec::new();
    let mut is_first_line = true;

    for segment in segments {
        match segment {
            ContentSegment::Text(text) => {
                // Process each line
                for line in text.lines() {
                    let line = line.trim();
                    if line.is_empty() {
                        // Add blank line for paragraph breaks
                        all_lines.push(Line::from(""));
                        continue;
                    }

                    // Check for heading
                    if let Some(level) = detect_heading_level(line) {
                        let heading_text = line.trim_start_matches('#').trim();
                        let base_style = heading_style(level, theme);
                        let prefix = if is_first_line {
                            ASSISTANT_PREFIX
                        } else {
                            CONTINUATION
                        };
                        let prefix_style = if is_first_line {
                            theme.assistant_prefix()
                        } else {
                            Style::default()
                        };

                        // Parse heading text for inline markdown (bold, italic, etc.)
                        let parsed_spans = parse_to_spans(heading_text, theme);
                        let mut line_spans = vec![Span::styled(prefix.to_string(), prefix_style)];

                        if parsed_spans.is_empty() {
                            line_spans.push(Span::styled(heading_text.to_string(), base_style));
                        } else {
                            for span in parsed_spans {
                                // Merge base heading style with inline style
                                let merged_style = base_style.patch(span.style);
                                line_spans
                                    .push(Span::styled(span.content.to_string(), merged_style));
                            }
                        }

                        all_lines.push(Line::from(line_spans));
                        is_first_line = false;
                        continue;
                    }

                    // Regular line - wrap with prefix
                    let prefix = if is_first_line {
                        ASSISTANT_PREFIX
                    } else {
                        CONTINUATION
                    };
                    let prefix_style = if is_first_line {
                        theme.assistant_prefix()
                    } else {
                        Style::default()
                    };

                    let lines = wrap_with_prefix(
                        line,
                        prefix,
                        prefix_style,
                        CONTINUATION,
                        max_width,
                        theme,
                    );
                    all_lines.extend(lines);
                    is_first_line = false;
                }
            }
            ContentSegment::Table(table_lines) => {
                let lines = render_table(&table_lines, theme);
                all_lines.extend(lines);
                is_first_line = false;
            }
            ContentSegment::CodeBlock { code, language: _ } => {
                let lines = render_code_block(&code, is_first_line, theme);
                all_lines.extend(lines);
                is_first_line = false;
            }
        }
    }
    all_lines
}

/// Render a code block with indentation and special styling
fn render_code_block(code: &str, is_first_line: bool, theme: &Theme) -> Vec<Line<'static>> {
    const CODE_INDENT: &str = "    "; // 4 spaces for code block indentation
    let code_style = theme.code_block();
    let prefix_style = theme.assistant_prefix();

    let mut lines = Vec::new();

    // Add a blank line before code block if not first
    if !is_first_line {
        lines.push(Line::from(""));
    }

    for (i, line) in code.lines().enumerate() {
        let mut spans = Vec::new();

        // First line of code block gets the assistant prefix, rest get continuation
        if i == 0 && is_first_line {
            spans.push(Span::styled(ASSISTANT_PREFIX, prefix_style));
        } else {
            spans.push(Span::raw(CONTINUATION));
        }

        // Add code indentation and the code line
        spans.push(Span::styled(format!("{}{}", CODE_INDENT, line), code_style));

        lines.push(Line::from(spans));
    }

    // Add a blank line after code block
    lines.push(Line::from(""));

    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plain_text() {
        let theme = Theme::default();
        let spans = parse_to_spans("hello world", &theme);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "hello world");
    }

    #[test]
    fn test_bold() {
        let theme = Theme::default();
        let spans = parse_to_spans("**bold**", &theme);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "bold");
        assert!(spans[0].style.add_modifier == Modifier::BOLD.into());
    }

    #[test]
    fn test_italic() {
        let theme = Theme::default();
        let spans = parse_to_spans("*italic*", &theme);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "italic");
    }

    #[test]
    fn test_mixed_formatting() {
        let theme = Theme::default();
        let spans = parse_to_spans("normal **bold** and *italic*", &theme);
        assert!(spans.len() >= 3);
    }

    #[test]
    fn test_inline_code() {
        let theme = Theme::default();
        let spans = parse_to_spans("use `code` here", &theme);
        assert!(spans.iter().any(|s| s.content == "code"));
    }

    #[test]
    fn test_styled_words() {
        let theme = Theme::default();
        let words = parse_to_styled_words("hello **bold** world", &theme);
        assert_eq!(words.len(), 3);
        assert_eq!(words[0].0, "hello");
        assert_eq!(words[1].0, "bold");
        assert_eq!(words[2].0, "world");
    }

    #[test]
    fn test_entirely_bold_line() {
        let theme = Theme::default();
        let input = "**The Midnight Adventure**";
        let spans = parse_to_spans(input, &theme);

        assert!(!spans.is_empty(), "Should have at least one span");
        assert!(
            spans[0].style.add_modifier.contains(Modifier::BOLD),
            "First span should be bold"
        );
    }

    #[test]
    fn test_link_parsing() {
        let theme = Theme::default();
        let input = "[The Rust Book](https://doc.rust-lang.org/book/)";
        let spans = parse_to_spans(input, &theme);

        assert!(
            spans.iter().any(|s| s.content.contains("Rust Book")),
            "Should contain link text"
        );
        assert!(
            spans
                .iter()
                .any(|s| s.content.contains("doc.rust-lang.org")),
            "Should contain URL"
        );
    }

    #[test]
    fn test_heading_detection() {
        // Valid headings
        assert_eq!(detect_heading_level("# Heading 1"), Some(1));
        assert_eq!(detect_heading_level("## Heading 2"), Some(2));
        assert_eq!(detect_heading_level("### Heading 3"), Some(3));
        assert_eq!(detect_heading_level("###### Heading 6"), Some(6));
        assert_eq!(detect_heading_level("# "), Some(1)); // Empty heading

        // Invalid headings
        assert_eq!(detect_heading_level("Not a heading"), None);
        assert_eq!(detect_heading_level("#NoSpace"), None); // No space after #
        assert_eq!(detect_heading_level("####### Too many"), None); // 7 hashes
    }

    #[test]
    fn test_render_markdown_with_indented_link() {
        let theme = Theme::default();
        let content = "Here is a link:\n    [The Rust Book](https://doc.rust-lang.org/book/)";
        let lines = render_markdown_with_prefix(content, 80, &theme);

        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect();

        assert!(
            all_text.contains("The Rust Book"),
            "Should contain link text"
        );
        assert!(
            !all_text.contains("](https://"),
            "URL should not appear in literal markdown syntax"
        );
    }

    #[test]
    fn test_styled_words_bold() {
        let theme = Theme::default();
        let words = parse_to_styled_words("**The Midnight Adventure**", &theme);
        assert_eq!(words.len(), 3);
        // All words should have BOLD
        for (word, style) in &words {
            assert!(
                style.add_modifier.contains(Modifier::BOLD),
                "Word {:?} should be bold",
                word
            );
        }
    }

    #[test]
    fn test_code_block_detection() {
        let content = "Some text\n```go\nfunc main() {\n    println(\"hello\")\n}\n```\nMore text";
        let segments = split_content_segments(content);

        assert_eq!(segments.len(), 3);

        // First segment is text
        match &segments[0] {
            ContentSegment::Text(t) => assert_eq!(t, "Some text"),
            _ => panic!("Expected Text segment"),
        }

        // Second segment is code block
        match &segments[1] {
            ContentSegment::CodeBlock { code, language } => {
                assert_eq!(language.as_deref(), Some("go"));
                assert!(code.contains("func main()"));
                assert!(code.contains("println"));
            }
            _ => panic!("Expected CodeBlock segment"),
        }

        // Third segment is text
        match &segments[2] {
            ContentSegment::Text(t) => assert_eq!(t, "More text"),
            _ => panic!("Expected Text segment"),
        }
    }

    #[test]
    fn test_code_block_no_language() {
        let content = "```\ncode here\n```";
        let segments = split_content_segments(content);

        assert_eq!(segments.len(), 1);
        match &segments[0] {
            ContentSegment::CodeBlock { code, language } => {
                assert!(language.is_none());
                assert_eq!(code, "code here");
            }
            _ => panic!("Expected CodeBlock segment"),
        }
    }
}
