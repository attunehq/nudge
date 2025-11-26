//! Source code snippet rendering with highlighted spans.
//!
//! This module provides utilities for displaying source code excerpts with
//! specific byte ranges highlighted, useful for showing where violations occur.
//!
//! # Example
//!
//! ```
//! use benchmark::snippet::Snippet;
//!
//! let source = "fn main() {\n    println!(\"hello\");\n}\n";
//! let snippet = Snippet::new(source);
//!
//! // Render with highlight at the same range as display
//! let rendered = snippet.render().display(16..31).highlight(16..31).finish();
//! println!("{rendered}");
//! ```

use std::fmt::{Formatter, Write};
use std::ops::Range;

use bon::bon;
use color_print::cformat;

/// Default number of context lines to show before and after the highlighted span.
pub const DEFAULT_CONTEXT_LINES: usize = 3;

/// A snippet of source content to be rendered.
///
/// Create snippet instances from the full source code (e.g. a full file) and
/// then use the methods on the type to render parts of the content.
#[derive(Debug, Clone)]
pub struct Snippet {
    /// The source content from which snippets can be rendered.
    source: String,
}

#[bon]
impl Snippet {
    /// Create a new snippet from a source content string.
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
        }
    }

    /// Get the originally provided source content.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Render the snippet with a highlighted range.
    ///
    /// Returns a builder that can be used to configure the rendering options.
    /// Call `.finish()` to get the final `RenderedSnippet`.
    ///
    /// If `highlight` extends beyond `display`, `display` will be automatically
    /// expanded to include the full highlighted range.
    #[builder(finish_fn = finish)]
    pub fn render(
        &self,
        /// The byte range to display.
        /// If not provided, the entire source content is displayed.
        #[builder(default = 0..self.self_receiver.source.len())]
        display: Range<usize>,
        /// The byte range to highlight.
        /// If not provided, no highlighting is done.
        #[builder(default = 0..0)]
        highlight: Range<usize>,
        /// The number of context lines to display before and after the displayed range.
        /// If not provided, defaults to [`DEFAULT_CONTEXT_LINES`].
        #[builder(default = DEFAULT_CONTEXT_LINES)]
        context_lines: usize,
        /// The indentation level (number of spaces) to use for the snippet.
        /// If not provided, no indentation is applied.
        #[builder(default = 0)]
        indent: usize,
    ) -> RenderedSnippet<'_> {
        // Expand display to include highlighted range if needed
        let display = if highlight.is_empty() {
            display
        } else {
            let start = display.start.min(highlight.start);
            let end = display.end.max(highlight.end);
            start..end
        };

        RenderedSnippet {
            source: &self.source,
            display,
            highlight,
            context_lines,
            indent,
        }
    }
}

/// A rendered snippet of source content, created by calling [`Snippet::render`].
#[derive(Debug, Clone)]
pub struct RenderedSnippet<'a> {
    /// The source content.
    source: &'a str,

    /// The byte range to display.
    display: Range<usize>,

    /// The byte range to highlight.
    highlight: Range<usize>,

    /// The number of context lines to display before and after the displayed range.
    context_lines: usize,

    /// The indentation level to use for the snippet.
    indent: usize,
}

impl<'a> RenderedSnippet<'a> {
    /// Get the source content.
    pub fn source(&self) -> &'a str {
        self.source
    }
}

impl std::fmt::Display for RenderedSnippet<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let lines = self.source.lines().collect::<Vec<_>>();
        if lines.is_empty() {
            return Ok(());
        }

        // Build a map of line index -> (start_byte, end_byte)
        let line_ranges = compute_line_ranges(self.source, &lines);

        // Find which lines the display range covers
        let display_start_line = find_line_for_byte(&line_ranges, self.display.start);
        let display_end_line = find_line_for_byte(&line_ranges, self.display.end.saturating_sub(1));

        // Calculate the window of lines to display (with context)
        let window_start = display_start_line.saturating_sub(self.context_lines);
        let window_end = (display_end_line + self.context_lines).min(lines.len() - 1);

        // Calculate the width needed for line numbers
        let line_num_width = (window_end + 1).to_string().len();

        // Build indent string
        let indent = " ".repeat(self.indent);

        // Render each line in the window
        for line_idx in window_start..=window_end {
            let line = lines[line_idx];
            let (line_start, line_end) = line_ranges[line_idx];

            // Format line number with indent
            f.write_str(&indent)?;
            let line_num = line_idx + 1;
            write!(f, "{}", cformat!("<dim>{line_num:>line_num_width$} │ </>"))?;

            // Render the line content with highlighting
            render_line_with_highlight(f, line, line_start, line_end, &self.highlight)?;
            f.write_char('\n')?;
        }

        Ok(())
    }
}

/// Compute the byte ranges for each line in the source.
fn compute_line_ranges(source: &str, lines: &[&str]) -> Vec<(usize, usize)> {
    let mut ranges = Vec::with_capacity(lines.len());
    let mut pos = 0;

    for line in lines {
        let start = pos;
        let end = pos + line.len();
        ranges.push((start, end));
        pos = end + 1; // +1 for the newline character
    }

    // Handle case where source doesn't end with newline
    if let Some(last) = ranges.last_mut() {
        last.1 = last.1.min(source.len());
    }

    ranges
}

/// Find which line contains a given byte offset.
fn find_line_for_byte(line_ranges: &[(usize, usize)], byte: usize) -> usize {
    for (i, (start, end)) in line_ranges.iter().enumerate() {
        if byte >= *start && byte <= *end {
            return i;
        }
    }
    // Default to last line if byte is past the end
    line_ranges.len().saturating_sub(1)
}

/// Render a single line with the span highlighted.
fn render_line_with_highlight(
    f: &mut Formatter<'_>,
    line: &str,
    line_start: usize,
    line_end: usize,
    highlight: &Range<usize>,
) -> std::fmt::Result {
    // Check if this line overlaps with the highlight at all
    if highlight.is_empty() || highlight.end <= line_start || highlight.start >= line_end {
        // No overlap - just output the line as-is
        f.write_str(line)?;
        return Ok(());
    }

    // Calculate the portion of this line that's highlighted
    let highlight_start = highlight.start.saturating_sub(line_start).min(line.len());
    let highlight_end = highlight.end.saturating_sub(line_start).min(line.len());

    // Output: [before][highlighted][after]
    let before = &line[..highlight_start];
    let highlighted = &line[highlight_start..highlight_end];
    let after = &line[highlight_end..];

    f.write_str(before)?;
    write!(f, "{}", cformat!("<bg:yellow,black>{highlighted}</>"))?;
    f.write_str(after)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_line_highlight() {
        let snippet = Snippet::new("hello world");
        let result = snippet
            .render()
            .display(0..11)
            .highlight(6..11)
            .context_lines(0)
            .finish()
            .to_string();

        // Should highlight "world"
        assert!(result.contains("world"));
    }

    #[test]
    fn test_multiline_highlight() {
        let snippet = Snippet::new("line one\nline two\nline three");
        let result = snippet
            .render()
            .display(5..15)
            .highlight(5..15)
            .context_lines(0)
            .finish()
            .to_string();

        // Span covers "one\nline t"
        assert!(result.contains("1 │"));
        assert!(result.contains("2 │"));
    }

    #[test]
    fn test_context_lines() {
        let snippet = Snippet::new("a\nb\nc\nd\ne\nf\ng");
        let result = snippet
            .render()
            .display(4..5)
            .highlight(4..5)
            .context_lines(2)
            .finish()
            .to_string();

        // Should show lines b, c, d, e (2 before, the line, 2 after)
        assert!(result.contains("2 │"));
        assert!(result.contains("3 │"));
        assert!(result.contains("4 │"));
        assert!(result.contains("5 │"));
    }

    #[test]
    fn test_highlight_expands_display() {
        let snippet = Snippet::new("line one\nline two\nline three");
        // Display only covers line 1, but highlight covers lines 1-2
        let result = snippet
            .render()
            .display(0..8)
            .highlight(5..15)
            .context_lines(0)
            .finish()
            .to_string();

        // Should show both lines because highlight expanded display
        assert!(result.contains("1 │"));
        assert!(result.contains("2 │"));
    }

    #[test]
    fn test_no_highlight() {
        let snippet = Snippet::new("hello world");
        let result = snippet
            .render()
            .display(0..11)
            .context_lines(0)
            .finish()
            .to_string();

        // Should just show the line without any highlighting
        assert!(result.contains("hello world"));
    }
}
