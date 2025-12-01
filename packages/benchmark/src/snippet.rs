//! Source code rendering types and helpers.

use std::{
    borrow::Cow,
    fmt::{Display, Formatter},
};

use bon::Builder;
use color_eyre::{Result, eyre::Context};
use color_print::{cformat, cwrite};
use extfn::extfn;
use tap::Pipe;
use tree_sitter::Node;

use crate::{
    ext::indent,
    matcher::{Span, code::Language},
};

/// Color options for highlighting spans in rendered output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HighlightColor {
    /// Yellow background - used for violations/errors.
    Yellow,
    /// Green background - used for successful matches.
    Green,
}

/// A snippet of source content to be rendered.
///
/// Create snippet instances from the full source code (e.g. a full file) and
/// then use the methods on the type to render parts of the content.
#[derive(Debug, Clone)]
pub struct Snippet<'a>(Cow<'a, str>);

impl<'a> Snippet<'a> {
    /// Create a new snippet from a source content string.
    pub fn new(source: impl Into<Cow<'a, str>>) -> Self {
        Self(source.into())
    }

    /// Get the originally provided source content.
    pub fn source(&self) -> &str {
        &self.0
    }

    /// Render the content of the snippet with the provided ranges highlighted.
    pub fn render_highlighted(
        &self,
        highlight: impl IntoIterator<Item = impl Into<Span>>,
    ) -> String {
        self.render_highlighted_with_color(highlight, HighlightColor::Yellow)
    }

    /// Render the content of the snippet with the provided ranges highlighted in green.
    pub fn render_highlighted_green(
        &self,
        highlight: impl IntoIterator<Item = impl Into<Span>>,
    ) -> String {
        self.render_highlighted_with_color(highlight, HighlightColor::Green)
    }

    /// Render the content of the snippet with the provided ranges highlighted in a specific color.
    fn render_highlighted_with_color(
        &self,
        highlight: impl IntoIterator<Item = impl Into<Span>>,
        color: HighlightColor,
    ) -> String {
        let mut source = self.0.to_string();

        // Collect and sort spans in reverse order (by start position descending).
        // This ensures replacements don't invalidate byte positions of earlier spans.
        let mut spans = highlight.into_iter().map(Into::into).collect::<Vec<Span>>();
        spans.sort_by(|a, b| b.start().cmp(&a.start()));

        // Merge overlapping spans. Since we're sorted descending by start,
        // we merge each span into the previous one if they overlap.
        let mut merged = Vec::<Span>::new();
        for span in spans {
            if let Some(last) = merged.last_mut() {
                if span.end() >= last.start() {
                    *last = Span::new(span.start(), last.end());
                    continue;
                }
            }
            merged.push(span);
        }

        for span in merged {
            let start = span.start();
            let end = span.end();
            let content = &source[start..end];
            // Highlight each line separately to prevent background color from
            // extending to end of line.
            let highlighted = content
                .split('\n')
                .map(|line| match color {
                    HighlightColor::Yellow => cformat!("<bg:yellow,black>{line}</>"),
                    HighlightColor::Green => cformat!("<bg:green,black>{line}</>"),
                })
                .collect::<Vec<_>>()
                .join("\n");
            source.replace_range(span.range(), &highlighted);
        }
        render_line_numbers(source)
    }

    /// Render the content of the snippet.
    pub fn render(&self) -> String {
        render_line_numbers(self.source())
    }

    /// Render the syntax tree of the snippet, parsed with the given language.
    ///
    /// This displays the source code with line numbers, followed by an indented
    /// tree view showing:
    /// - Field name (if present), e.g. `name:`, `body:`
    /// - Node kind (what you'd match in a query)
    /// - Position as `[line:col-line:col]` (1-indexed)
    /// - Source text for small nodes
    ///
    /// This is useful for understanding the tree structure when writing
    /// tree-sitter queries.
    pub fn render_syntax_tree(&self, language: Language) -> Result<String> {
        let source = self.source();
        let tree = language.parse_code(source).context("parse syntax tree")?;
        tree.root_node()
            .to_syntax_nodes(source, 0, None)
            .pipe_as_ref(RenderSyntaxTree)
            .to_string()
            .pipe(Ok)
    }
}

/// A parsed syntax node from the syntax tree.
///
/// Nodes are represented in a tree structure in treesitter, but this struct
/// flattens them into a linear structure using `depth` and the order of this
/// node inside the parent container. It's intended to be used for rendering.
///
/// ## Rendering with `Display`
///
/// The default `Display` implementation renders the node with ignoring its
/// `span` and `depth` fields. If you plan to render multiple nodes, you
/// probably want to wrap your collection in [`RenderSyntaxTree`] for display.
#[derive(Debug, Clone, Builder)]
#[non_exhaustive]
pub struct SyntaxNode<'a> {
    /// The span of the overall node in the source code.
    ///
    /// `text` is the equivalent of selecting `span` from the source code.
    #[builder(into)]
    pub span: Span,

    /// The depth of the node in the syntax tree.
    ///
    /// This is mainly intended to be used for indentation when rendering, but
    /// it can technically also be used to reconstruct the tree structure in
    /// combination with the ordering of nodes within a parent container.
    pub depth: usize,

    /// The kind of the node.
    ///
    /// This is the type of the node, as defined in the treesitter grammar.
    /// For example, `"function_item"` or `"identifier"`.
    #[builder(into)]
    pub kind: String,

    /// The field name of the node.
    ///
    /// This is the name of the field that this node is a child of, if any.
    /// For example, if the node is a child of a `function_item`, it might
    /// contain the fields `name`, `parameters`, `return_type`, and `body`.
    #[builder(into)]
    pub field_name: Option<String>,

    /// The text of the node.
    ///
    /// This is the text content of the node. Note that parent nodes contain the
    /// full text content of all their children: e.g. `function_item` contains
    /// the full text of the function it declares even though it has children
    /// that contain smaller sections of the function body.
    #[builder(default = "")]
    pub text: &'a str,
}

impl<'a> Display for SyntaxNode<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let field = self
            .field_name
            .as_deref()
            .map(|name| format!("{name}: "))
            .unwrap_or_default();
        let kind = self.kind.as_str();
        let text = self.text;
        if text.len() <= 40 && !text.contains('\n') {
            cwrite!(f, "<cyan>{field}</><green>{kind}</> <dim>{text}</>")
        } else {
            cwrite!(f, "<cyan>{field}</><green>{kind}</> <dim>..</>")
        }
    }
}

/// Wrap a collection of syntax nodes for rendering in a tree structure,
/// preserving the original order and indentation.
#[derive(Debug, Clone)]
pub struct RenderSyntaxTree<'n, 'c>(pub &'c [SyntaxNode<'n>]);

impl<'n, 'c> RenderSyntaxTree<'n, 'c> {
    /// Create a new render syntax tree from a vector of syntax nodes.
    pub fn new(nodes: &'c [SyntaxNode<'n>]) -> Self {
        Self(nodes)
    }
}

impl<'n, 'c> Display for RenderSyntaxTree<'n, 'c> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        for node in self.0 {
            writeln!(f, "{}", node.to_string().indent(node.depth * 2))?;
        }
        Ok(())
    }
}

/// Recursively build a flat collection of syntax nodes from a treesitter node.
#[extfn]
fn to_syntax_nodes<'a>(
    self: Node<'_>,
    source: &'a str,
    depth: usize,
    field_name: Option<&str>,
) -> Vec<SyntaxNode<'a>> {
    let mut nodes = Vec::new();

    // let pos = format!(
    //     "[{:}:{:}-{:}:{:}]",
    //     start.row + 1,
    //     start.column + 1,
    //     end.row + 1,
    //     end.column + 1
    // );

    // We only show text for smaller nodes that don't contain newlines, because
    // this is meant to help users identify how parsed nodes correspond to
    // source code and in treesitter many nodes contain the content of their
    // children.
    // let text = self
    //     .utf8_text(source.as_bytes())
    //     .ok()
    //     .filter(|t| t.len() <= 40 && !t.contains('\n'))
    //     .map(|t| format!(" {t:?}"))
    //     .unwrap_or_default();

    // Format with optional field name prefix
    // let field_prefix = field_name.map(|f| format!("{f}: ")).unwrap_or_default();
    // let position = cformat!("<dim>{pos}</> ");
    // let node = cformat!("<cyan>{field_prefix}</><green>{kind}</> {text}\n").indent(depth * 2);
    // output.push_str(&position);
    // output.push_str(&node);

    let node = self.to_syntax_node(source, depth, field_name);
    nodes.push(node);

    let mut cursor = self.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            let field_name = cursor.field_name();
            let children = child.to_syntax_nodes(source, depth + 1, field_name);
            nodes.extend(children);

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
    nodes
}

/// Convert a treesitter node to a syntax node.
#[extfn]
fn to_syntax_node<'a>(
    self: Node<'_>,
    source: &'a str,
    depth: usize,
    field_name: Option<&str>,
) -> SyntaxNode<'a> {
    SyntaxNode::builder()
        .span(self.byte_range())
        .depth(depth)
        .kind(self.kind())
        .maybe_field_name(field_name)
        .text(self.utf8_text(source.as_bytes()).unwrap_or_default())
        .build()
}

fn render_line_numbers(source: impl AsRef<str>) -> String {
    let source = source.as_ref();
    let lines = source.lines().zip(1..).collect::<Vec<_>>();
    let max_width = lines.len().to_string().len();
    lines
        .iter()
        .map(|(line, number)| cformat!("<dim>{number:>max_width$} |</> {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}
