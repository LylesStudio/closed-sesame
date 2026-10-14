//! Multi-select path helpers for the file browser.

use crate::container::path::is_prefix;
use std::collections::BTreeSet;

pub(crate) fn dedupe_delete_paths(paths: &BTreeSet<String>) -> Vec<String> {
    let mut result: Vec<String> = paths.iter().cloned().collect();
    result.sort_by_key(|path| path.len());
    let snapshot = result.clone();
    result.retain(|path| {
        !snapshot
            .iter()
            .any(|other| other != path && is_prefix(other, path))
    });
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn dedupe_removes_child_when_parent_selected() {
        let mut paths = BTreeSet::new();
        paths.insert("foo".into());
        paths.insert("foo/bar.txt".into());
        let deduped = dedupe_delete_paths(&paths);
        assert_eq!(deduped, vec!["foo".to_string()]);
    }

    #[test]
    fn dedupe_keeps_siblings() {
        let mut paths = BTreeSet::new();
        paths.insert("a.txt".into());
        paths.insert("b.txt".into());
        let deduped = dedupe_delete_paths(&paths);
        assert_eq!(deduped.len(), 2);
        assert!(deduped.contains(&"a.txt".to_string()));
        assert!(deduped.contains(&"b.txt".to_string()));
    }
}
