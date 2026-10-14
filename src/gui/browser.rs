//! Tree model for hierarchical vault browsing.

use crate::container::FileEntry;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default)]
pub struct BrowserNode {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    /// Unix seconds; for directories this is the newest child's time.
    pub modified_at: u64,
    pub children: BTreeMap<String, BrowserNode>,
}

impl BrowserNode {
    /// Number of direct children that are files / directories.
    pub fn counts(&self) -> (usize, usize) {
        let dirs = self.children.values().filter(|child| child.is_dir).count();
        (self.children.len() - dirs, dirs)
    }

    /// Total bytes of all files below this node.
    pub fn total_size(&self) -> u64 {
        if self.is_dir {
            self.children.values().map(BrowserNode::total_size).sum()
        } else {
            self.size
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct BrowserTree {
    pub root: BrowserNode,
}

impl BrowserTree {
    pub fn from_entries(entries: &[FileEntry]) -> Self {
        let mut root = BrowserNode {
            name: String::new(),
            path: String::new(),
            is_dir: true,
            size: 0,
            modified_at: 0,
            children: BTreeMap::new(),
        };
        for entry in entries {
            insert_entry(&mut root, entry);
        }
        Self { root }
    }

    pub fn node_at(&self, path: &str) -> &BrowserNode {
        if path.is_empty() {
            return &self.root;
        }
        let mut current = &self.root;
        for component in path.split('/') {
            current = current.children.get(component).unwrap_or(current);
        }
        current
    }

    /// Whether `path` resolves to an existing directory node.
    pub fn has_dir(&self, path: &str) -> bool {
        if path.is_empty() {
            return true;
        }
        let mut current = &self.root;
        for component in path.split('/') {
            match current.children.get(component) {
                Some(child) if child.is_dir => current = child,
                _ => return false,
            }
        }
        true
    }

    /// Every node whose name contains `query` (case-insensitive), at any depth.
    pub fn search<'a>(&'a self, query: &str) -> Vec<&'a BrowserNode> {
        let mut hits = Vec::new();
        if !query.is_empty() {
            walk_search(&self.root, query, &mut hits);
        }
        hits
    }

    /// Deepest existing ancestor directory of `path` (possibly the root).
    pub fn nearest_existing_dir(&self, path: &str) -> String {
        let mut candidate = path.to_owned();
        loop {
            if self.has_dir(&candidate) {
                return candidate;
            }
            match candidate.rsplit_once('/') {
                Some((parent, _)) => candidate = parent.to_owned(),
                None => return String::new(),
            }
        }
    }
}

fn walk_search<'a>(node: &'a BrowserNode, query: &str, hits: &mut Vec<&'a BrowserNode>) {
    for child in node.children.values() {
        if child.name.to_lowercase().contains(query) {
            hits.push(child);
        }
        if child.is_dir {
            walk_search(child, query, hits);
        }
    }
}

fn insert_entry(root: &mut BrowserNode, entry: &FileEntry) {
    let parts: Vec<_> = entry.path.split('/').collect();
    let mut current = root;
    for (index, component) in parts.iter().enumerate() {
        let is_last = index + 1 == parts.len();
        let path = parts[..=index].join("/");
        current.modified_at = current.modified_at.max(entry.modified_at);
        current = current
            .children
            .entry((*component).to_string())
            .or_insert_with(|| BrowserNode {
                name: (*component).to_string(),
                path: path.clone(),
                is_dir: !is_last || entry.is_directory,
                size: 0,
                modified_at: 0,
                children: BTreeMap::new(),
            });
        if is_last {
            current.is_dir = entry.is_directory;
            current.size = entry.size;
            current.path = entry.path.clone();
            current.modified_at = current.modified_at.max(entry.modified_at);
        } else {
            current.is_dir = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, is_dir: bool, size: u64, modified_at: u64) -> FileEntry {
        FileEntry {
            path: path.into(),
            original_path: None,
            size,
            start_page: 0,
            page_count: 0,
            crc32: 0,
            created_at: 0,
            modified_at,
            has_symlink: false,
            is_directory: is_dir,
        }
    }

    #[test]
    fn nearest_existing_dir_falls_back_to_ancestor() {
        let tree = BrowserTree::from_entries(&[
            entry("docs", true, 0, 1),
            entry("docs/a.txt", false, 3, 5),
        ]);
        assert!(tree.has_dir("docs"));
        assert!(!tree.has_dir("docs/a.txt"));
        assert_eq!(tree.nearest_existing_dir("docs/gone/deeper"), "docs");
        assert_eq!(tree.nearest_existing_dir("nope"), "");
        assert_eq!(tree.root.modified_at, 5);
        assert_eq!(tree.node_at("docs").total_size(), 3);
        let hits = tree.search("a.t");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, "docs/a.txt");
    }
}
