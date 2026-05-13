use std::fmt;
use std::path::{Path, PathBuf};

use colored::Colorize;

/// A single row rendered inside a [`PolicyNotice`].
enum Row {
    /// A bold label followed by a plain value on the same line. If `value` is
    /// empty the label is rendered alone.
    KeyValue { label: String, value: String },
    /// A bold label on one line followed by a dimmed OSC 8 clickable URL on
    /// the next line.
    Docs { label: String, url: String },
    /// A bold label followed by a comma-separated, truncated item list.
    Items { label: String, items: Vec<String>, max_display: usize },
}

impl Row {
    fn render(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Row::KeyValue { label, value } if value.is_empty() => {
                write!(f, "  {}", label.bold())
            }
            Row::KeyValue { label, value } => {
                write!(f, "  {} {value}", label.bold())
            }
            Row::Docs { label, url } => {
                let link = format!("\x1b]8;;{url}\x1b\\{url}\x1b]8;;\x1b\\");
                write!(f, "  {} {}", label.bold(), link.dimmed())
            }
            Row::Items { label, items, max_display } => {
                let shown = items
                    .iter()
                    .take(*max_display)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ");
                let list = if items.len() > *max_display {
                    format!("{shown} +{} more", items.len() - max_display)
                } else {
                    shown
                };
                write!(f, "  {} {list}", label.bold())
            }
        }
    }
}

/// A composable terminal notice for policy-blocked items.
///
/// Build up any combination of key-value rows, docs hyperlink rows, and
/// truncated item-list rows in any order. The `Display` impl renders each row
/// indented with bold labels.
///
/// # Example
///
/// ```rust,ignore
/// let notice = PolicyNotice::new()
///     .row("To enable them, configure", tilde_path(&permissions_path))
///     .docs("Learn how to configure permissions:", "https://forgecode.dev/docs/permissions/")
///     .items("Blocked servers:", server_names, 3);
/// ```
#[derive(Default)]
pub struct PolicyNotice {
    rows: Vec<Row>,
}

impl PolicyNotice {
    /// Creates an empty notice.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a bold-label + plain-value row. Pass an empty string as `value`
    /// to render the label alone (e.g. as a section header).
    pub fn row(mut self, label: impl Into<String>, value: impl Into<String>) -> Self {
        self.rows.push(Row::KeyValue { label: label.into(), value: value.into() });
        self
    }

    /// Appends a bold label on one line followed by a dimmed OSC 8 clickable
    /// URL on the next line. Position in the output respects insertion order.
    pub fn docs(mut self, label: impl Into<String>, url: impl Into<String>) -> Self {
        self.rows.push(Row::Docs { label: label.into(), url: url.into() });
        self
    }

    /// Appends a bold-label + truncated item-list row.
    pub fn items(
        mut self,
        label: impl Into<String>,
        items: Vec<String>,
        max_display: usize,
    ) -> Self {
        self.rows.push(Row::Items { label: label.into(), items, max_display });
        self
    }
}

impl fmt::Display for PolicyNotice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;
        for row in &self.rows {
            if !first {
                writeln!(f)?;
            }
            row.render(f)?;
            first = false;
        }
        Ok(())
    }
}

/// Abbreviates a path by replacing the home directory prefix with `~`.
pub fn tilde_path(path: &PathBuf) -> String {
    if let Ok(home) = std::env::var("HOME") {
        let home_path = Path::new(&home);
        path.strip_prefix(home_path)
            .map(|p| format!("~/{}", p.display()))
            .unwrap_or_else(|_| path.display().to_string())
    } else {
        path.display().to_string()
    }
}
