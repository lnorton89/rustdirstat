// ============================================================================
// Module:       gui::app::extensions
// Description:  The extension breakdown, the flat file views, and the column
//               order shared by both tables.
//
// Dependencies: crate::{color, stats}; super::GuiApp
// ============================================================================

//! The extension breakdown, the flat file views, and the column order
//! shared by both tables.

use super::*;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::gui) enum DirectoryColumn {
    Name,
    Size,
    SubtreePercentage,
    PercentTotal,
    Files,
    Subdirs,
    LastChange,
    Attributes,
}

impl DirectoryColumn {
    pub(in crate::gui) const DEFAULT_ORDER: [Self; 8] = [
        Self::Name,
        Self::Size,
        Self::SubtreePercentage,
        Self::PercentTotal,
        Self::Files,
        Self::Subdirs,
        Self::LastChange,
        Self::Attributes,
    ];
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::gui) enum ExtensionColumn {
    Extension,
    Color,
    Description,
    Bytes,
    PercentBytes,
    Files,
}

impl ExtensionColumn {
    pub(in crate::gui) const DEFAULT_ORDER: [Self; 6] = [
        Self::Extension,
        Self::Color,
        Self::Description,
        Self::Bytes,
        Self::PercentBytes,
        Self::Files,
    ];
}

#[derive(Clone)]
pub(in crate::gui) struct ExtensionRow {
    pub extension: String,
    pub category: Category,
    pub size: u64,
    pub count: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::gui) enum ExtensionSortMode {
    ExtensionAsc,
    ExtensionDesc,
    ColorAsc,
    ColorDesc,
    DescriptionAsc,
    DescriptionDesc,
    BytesDesc,
    BytesAsc,
    PercentDesc,
    PercentAsc,
    FilesDesc,
    FilesAsc,
}

/// Sorting the extension table by its Color column orders rows by the
/// hue they are actually painted at, so it has to be the same hue.
pub(in crate::gui) fn extension_color_sort_key(extension: &str) -> u32 {
    crate::color::extension_hue(extension) as u32
}

pub(in crate::gui) fn reorder_column<T: Copy + Eq>(columns: &mut Vec<T>, source: T, target: T) {
    if source == target {
        return;
    }
    let Some(source_index) = columns.iter().position(|column| *column == source) else {
        return;
    };
    columns.remove(source_index);
    let Some(target_index) = columns.iter().position(|column| *column == target) else {
        columns.insert(source_index.min(columns.len()), source);
        return;
    };
    columns.insert(target_index, source);
}

pub(in crate::gui) fn extension_label(name: &str) -> String {
    Path::new(name)
        .extension()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .map(|s| format!(".{}", s.to_ascii_lowercase()))
        .unwrap_or_else(|| NO_EXTENSION_LABEL.to_string())
}

/// One row per distinct extension anywhere under `node`.
///
/// Free of `GuiApp` so the scan thread can call it before the tree is
/// ever handed over — see [`ScanOutcome`].
pub(in crate::gui) fn collect_extension_rows(node: &Node, physical: bool) -> Vec<ExtensionRow> {
    let mut by_ext: HashMap<String, (Category, u64, u64)> = HashMap::new();
    collect_extensions(node, physical, &mut by_ext);
    by_ext
        .into_iter()
        .map(|(extension, (category, size, count))| ExtensionRow {
            extension,
            category,
            size,
            count,
        })
        .collect()
}

pub(in crate::gui) fn collect_extensions(
    node: &Node,
    physical: bool,
    out: &mut HashMap<String, (Category, u64, u64)>,
) {
    for child in &node.children {
        if child.is_dir {
            collect_extensions(child, physical, out);
        } else {
            // Display-only territory: the legend's labels. The raw bytes
            // stay on the node; `category_for_name` takes them as-is.
            let extension = extension_label(&child.name.to_string_lossy());
            let category = child
                .category
                .unwrap_or_else(|| category_for_name(&child.name));
            let entry = out.entry(extension).or_insert((category, 0, 0));
            entry.1 = entry.1.saturating_add(child.effective_size(physical));
            entry.2 = entry.2.saturating_add(1);
        }
    }
}

pub(in crate::gui) fn size_label(bytes: u64, physical: bool) -> String {
    format!(
        "{}{}",
        human_bytes(bytes),
        if physical { " (physical)" } else { "" }
    )
}

impl GuiApp {
    pub(in crate::gui) fn refresh_extensions(&mut self) {
        self.extensions = collect_extension_rows(self.zoom_node(), self.use_physical);
        self.sort_extensions();
    }

    pub(in crate::gui) fn sort_extensions(&mut self) {
        let by_extension = |a: &ExtensionRow, b: &ExtensionRow| {
            a.extension.to_lowercase().cmp(&b.extension.to_lowercase())
        };
        match self.extension_sort {
            ExtensionSortMode::ExtensionAsc => self.extensions.sort_by(by_extension),
            ExtensionSortMode::ExtensionDesc => self.extensions.sort_by(|a, b| by_extension(b, a)),
            ExtensionSortMode::ColorAsc => self.extensions.sort_by(|a, b| {
                extension_color_sort_key(&a.extension)
                    .cmp(&extension_color_sort_key(&b.extension))
                    .then_with(|| by_extension(a, b))
            }),
            ExtensionSortMode::ColorDesc => self.extensions.sort_by(|a, b| {
                extension_color_sort_key(&b.extension)
                    .cmp(&extension_color_sort_key(&a.extension))
                    .then_with(|| by_extension(a, b))
            }),
            ExtensionSortMode::DescriptionAsc => self.extensions.sort_by(|a, b| {
                a.category
                    .label()
                    .cmp(b.category.label())
                    .then_with(|| by_extension(a, b))
            }),
            ExtensionSortMode::DescriptionDesc => self.extensions.sort_by(|a, b| {
                b.category
                    .label()
                    .cmp(a.category.label())
                    .then_with(|| by_extension(a, b))
            }),
            ExtensionSortMode::BytesDesc => self
                .extensions
                .sort_by(|a, b| b.size.cmp(&a.size).then_with(|| by_extension(a, b))),
            ExtensionSortMode::BytesAsc => self
                .extensions
                .sort_by(|a, b| a.size.cmp(&b.size).then_with(|| by_extension(a, b))),
            ExtensionSortMode::PercentDesc => self
                .extensions
                .sort_by(|a, b| b.size.cmp(&a.size).then_with(|| by_extension(a, b))),
            ExtensionSortMode::PercentAsc => self
                .extensions
                .sort_by(|a, b| a.size.cmp(&b.size).then_with(|| by_extension(a, b))),
            ExtensionSortMode::FilesDesc => self
                .extensions
                .sort_by(|a, b| b.count.cmp(&a.count).then_with(|| by_extension(a, b))),
            ExtensionSortMode::FilesAsc => self
                .extensions
                .sort_by(|a, b| a.count.cmp(&b.count).then_with(|| by_extension(a, b))),
        }
    }

    pub(in crate::gui) fn reorder_directory_column(
        &mut self,
        source: DirectoryColumn,
        target: DirectoryColumn,
    ) {
        reorder_column(&mut self.directory_column_order, source, target);
    }

    pub(in crate::gui) fn reorder_extension_column(
        &mut self,
        source: ExtensionColumn,
        target: ExtensionColumn,
    ) {
        reorder_column(&mut self.extension_column_order, source, target);
    }

    pub(in crate::gui) fn refresh_largest_files(&mut self) {
        self.largest_files = top_files::top_k(&self.tree.root, 200);
    }

    pub(in crate::gui) fn run_search(&mut self) {
        let outcome = search::search(&self.tree.root, &self.search.query);
        self.search.results = outcome.hits;
        self.search.error = outcome.error;
        self.file_view = FileView::SearchResults;
        self.status = Some(if outcome.truncated {
            "Search capped at 2,000 results".to_string()
        } else {
            format!("{} search result(s)", self.search.results.len())
        });
    }
}
