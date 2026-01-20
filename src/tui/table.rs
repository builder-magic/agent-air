//! Table rendering utilities for TUI
//!
//! Supports multiple rendering strategies via trait.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use unicode_width::UnicodeWidthStr;

use super::themes::Theme;

/// Indent prefix for table lines
const TABLE_INDENT: &str = "  ";

/// Trait for table rendering strategies
pub trait TableRenderer {
    fn render(&self, table_lines: &[String], theme: &Theme) -> Vec<Line<'static>>;
}

/// Renders tables using pulldown-cmark parsing
///
/// Provides proper CommonMark table parsing with markdown support in cells
pub struct PulldownRenderer;

/// A styled segment within a cell
type StyledSegment = (String, Style);
/// A cell is a list of styled segments
type StyledCell = Vec<StyledSegment>;
/// A row is a list of cells
type StyledRow = Vec<StyledCell>;

impl TableRenderer for PulldownRenderer {
    fn render(&self, table_lines: &[String], theme: &Theme) -> Vec<Line<'static>> {
        // Join lines back into markdown text
        let markdown = table_lines.join("\n");

        // Parse with table support enabled
        let mut options = Options::empty();
        options.insert(Options::ENABLE_TABLES);
        let parser = Parser::new_ext(&markdown, options);

        // Extract table structure from events
        let mut rows: Vec<StyledRow> = Vec::new();
        let mut current_row: StyledRow = Vec::new();
        let mut current_cell: StyledCell = Vec::new();
        let mut current_text = String::new();
        let mut style_stack: Vec<Modifier> = Vec::new();
        let mut color_stack: Vec<Color> = Vec::new();
        let mut header_row_count = 0;

        for event in parser {
            match event {
                Event::Start(Tag::TableHead) => {
                    current_row = Vec::new();
                }
                Event::End(TagEnd::TableHead) => {
                    if !current_row.is_empty() {
                        rows.push(current_row.clone());
                        header_row_count += 1;
                    }
                    current_row = Vec::new();
                }
                Event::Start(Tag::TableRow) => {
                    current_row = Vec::new();
                }
                Event::End(TagEnd::TableRow) => {
                    if !current_row.is_empty() {
                        rows.push(current_row.clone());
                    }
                    current_row = Vec::new();
                }
                Event::Start(Tag::TableCell) => {
                    current_cell = Vec::new();
                    current_text.clear();
                    style_stack.clear();
                    color_stack.clear();
                }
                Event::End(TagEnd::TableCell) => {
                    // Flush any remaining text
                    if !current_text.is_empty() {
                        let style = build_cell_style(&style_stack, &color_stack);
                        current_cell.push((current_text.trim().to_string(), style));
                        current_text.clear();
                    }
                    current_row.push(current_cell.clone());
                    current_cell = Vec::new();
                }
                // Formatting events
                Event::Start(Tag::Strong) => {
                    flush_text(&mut current_text, &mut current_cell, &style_stack, &color_stack);
                    style_stack.push(theme.bold());
                }
                Event::End(TagEnd::Strong) => {
                    flush_text(&mut current_text, &mut current_cell, &style_stack, &color_stack);
                    style_stack.pop();
                }
                Event::Start(Tag::Emphasis) => {
                    flush_text(&mut current_text, &mut current_cell, &style_stack, &color_stack);
                    style_stack.push(theme.italic());
                }
                Event::End(TagEnd::Emphasis) => {
                    flush_text(&mut current_text, &mut current_cell, &style_stack, &color_stack);
                    style_stack.pop();
                }
                Event::Start(Tag::Strikethrough) => {
                    flush_text(&mut current_text, &mut current_cell, &style_stack, &color_stack);
                    style_stack.push(theme.strikethrough());
                }
                Event::End(TagEnd::Strikethrough) => {
                    flush_text(&mut current_text, &mut current_cell, &style_stack, &color_stack);
                    style_stack.pop();
                }
                Event::Code(code) => {
                    flush_text(&mut current_text, &mut current_cell, &style_stack, &color_stack);
                    current_cell.push((code.to_string(), theme.inline_code()));
                }
                Event::Text(text) => {
                    current_text.push_str(&text);
                }
                Event::SoftBreak | Event::HardBreak => {
                    current_text.push(' ');
                }
                _ => {}
            }
        }

        if rows.is_empty() {
            return Vec::new();
        }

        // Calculate column widths (sum of all segment lengths per cell)
        let col_count = rows.iter().map(|r| r.len()).max().unwrap_or(0);
        let mut col_widths: Vec<usize> = vec![0; col_count];

        for row in &rows {
            for (i, cell) in row.iter().enumerate() {
                if i < col_widths.len() {
                    let cell_width: usize = cell.iter().map(|(s, _)| s.width()).sum();
                    col_widths[i] = col_widths[i].max(cell_width);
                }
            }
        }

        // Render using box-drawing style
        let mut lines = Vec::new();
        let header_style = theme.table_header();
        let cell_style = theme.table_cell();
        let border_style = theme.table_border();

        // Top border
        lines.push(render_border(&col_widths, '\u{250C}', '\u{252C}', '\u{2510}', border_style));

        // Header rows
        let header_count = header_row_count.max(1).min(rows.len());
        for row in rows.iter().take(header_count) {
            lines.push(render_styled_row(row, &col_widths, header_style, border_style));
        }

        // Separator after header
        if rows.len() > header_count {
            lines.push(render_border(&col_widths, '\u{251C}', '\u{253C}', '\u{2524}', border_style));
        }

        // Data rows
        for row in rows.iter().skip(header_count) {
            lines.push(render_styled_row(row, &col_widths, cell_style, border_style));
        }

        // Bottom border
        lines.push(render_border(&col_widths, '\u{2514}', '\u{2534}', '\u{2518}', border_style));

        lines
    }
}

/// Flush accumulated text to cell with current style
fn flush_text(
    current_text: &mut String,
    current_cell: &mut StyledCell,
    style_stack: &[Modifier],
    color_stack: &[Color],
) {
    if !current_text.is_empty() {
        let style = build_cell_style(style_stack, color_stack);
        current_cell.push((current_text.clone(), style));
        current_text.clear();
    }
}

/// Build a Style from modifier and color stacks
fn build_cell_style(modifiers: &[Modifier], colors: &[Color]) -> Style {
    let mut style = Style::default();
    for modifier in modifiers {
        style = style.add_modifier(*modifier);
    }
    if let Some(&color) = colors.last() {
        style = style.fg(color);
    }
    style
}

/// Render a table row with styled cells
fn render_styled_row(
    cells: &[StyledCell],
    col_widths: &[usize],
    base_style: Style,
    border_style: Style,
) -> Line<'static> {
    let mut spans = vec![
        Span::raw(TABLE_INDENT),
        Span::styled("\u{2502}", border_style),
    ];

    for (i, width) in col_widths.iter().enumerate() {
        spans.push(Span::styled(" ", base_style)); // left padding

        let mut cell_len = 0;
        if let Some(cell) = cells.get(i) {
            for (text, style) in cell {
                // Merge base_style with segment style (segment style takes precedence)
                let merged = base_style.patch(*style);
                spans.push(Span::styled(text.clone(), merged));
                cell_len += text.width();
            }
        }

        // Right padding to fill column width
        let padding = width.saturating_sub(cell_len) + 1;
        spans.push(Span::styled(" ".repeat(padding), base_style));
        spans.push(Span::styled("\u{2502}", border_style));
    }

    Line::from(spans)
}

// ============================================================================
// Public API
// ============================================================================

/// Check if a line could be part of a table (contains |)
pub fn is_table_line(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.contains('|')
}

/// Check if a line is a table separator (|---|---|)
///
/// Must be primarily dashes, pipes, colons, and spaces - not content with hyphens
pub fn is_table_separator(line: &str) -> bool {
    let trimmed = line.trim();
    if !trimmed.contains('-') || !trimmed.contains('|') {
        return false;
    }
    // A separator line should be mostly dashes, pipes, colons, and spaces
    let non_sep_chars = trimmed
        .chars()
        .filter(|c| !matches!(c, '-' | '|' | ':' | ' '))
        .count();
    non_sep_chars == 0
}

/// Render a table using PulldownRenderer
pub fn render_table(table_lines: &[String], theme: &Theme) -> Vec<Line<'static>> {
    PulldownRenderer.render(table_lines, theme)
}

// ============================================================================
// Internal helpers
// ============================================================================

/// Render a table border line (with indent)
fn render_border(
    col_widths: &[usize],
    left: char,
    mid: char,
    right: char,
    style: Style,
) -> Line<'static> {
    let mut content = String::new();
    content.push(left);
    for (i, &width) in col_widths.iter().enumerate() {
        content.push_str(&"\u{2500}".repeat(width + 2)); // +2 for cell padding
        if i < col_widths.len() - 1 {
            content.push(mid);
        }
    }
    content.push(right);

    Line::from(vec![
        Span::raw(TABLE_INDENT),
        Span::styled(content, style),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_table_line() {
        assert!(is_table_line("| a | b |"));
        assert!(is_table_line("a | b"));
        assert!(!is_table_line("no pipes here"));
    }

    #[test]
    fn test_is_table_separator() {
        assert!(is_table_separator("|---|---|"));
        assert!(is_table_separator("| --- | --- |"));
        assert!(is_table_separator("|:---:|:---:|"));
        assert!(!is_table_separator("| F-150 | truck |")); // content with hyphen
        assert!(!is_table_separator("no pipes"));
    }

    #[test]
    fn test_render_table_empty() {
        let theme = Theme::default();
        let lines = render_table(&[], &theme);
        assert!(lines.is_empty());
    }

    #[test]
    fn test_render_table_basic() {
        let theme = Theme::default();
        let table_lines = vec![
            "| Name | Age |".to_string(),
            "|------|-----|".to_string(),
            "| Alice | 30 |".to_string(),
        ];
        let lines = render_table(&table_lines, &theme);
        // Should have: top border, header, separator, 1 data row, bottom border = 5 lines
        assert_eq!(lines.len(), 5);
    }

    #[test]
    fn test_pulldown_renderer_basic() {
        let theme = Theme::default();
        let table_lines = vec![
            "| Name | Age |".to_string(),
            "|------|-----|".to_string(),
            "| Alice | 30 |".to_string(),
        ];
        let lines = PulldownRenderer.render(&table_lines, &theme);
        assert_eq!(lines.len(), 5);
    }

    #[test]
    fn test_pulldown_renderer_multiple_rows() {
        let theme = Theme::default();
        let table_lines = vec![
            "| Product | Price | Stock |".to_string(),
            "|---------|-------|-------|".to_string(),
            "| Apple | $1.00 | 50 |".to_string(),
            "| Banana | $0.50 | 100 |".to_string(),
        ];
        let lines = PulldownRenderer.render(&table_lines, &theme);
        // top border, header, separator, 2 data rows, bottom border = 6 lines
        assert_eq!(lines.len(), 6);
    }

    #[test]
    fn test_pulldown_renderer_styled_cells() {
        let theme = Theme::default();
        let table_lines = vec![
            "| **Name** | Age |".to_string(),
            "|----------|-----|".to_string(),
            "| *Alice* | 30 |".to_string(),
            "| `Bob` | 25 |".to_string(),
        ];
        let lines = PulldownRenderer.render(&table_lines, &theme);
        assert_eq!(lines.len(), 6);

        // The row should contain multiple spans (border + styled cells)
        let data_row = &lines[3];
        assert!(data_row.spans.len() > 3, "Data row should have multiple spans for styling");
    }
}
