//! Drag a vault entry onto a folder to move it.

use crate::container::path::is_prefix;
use std::collections::BTreeSet;

/// Paths picked up from a row. A drag that starts on a selected row carries
/// the whole selection; a drag that starts elsewhere carries just that row.
#[derive(Clone, Debug)]
pub(crate) struct MovePayload {
    pub(crate) paths: Vec<String>,
}

pub(crate) fn paths_for_drag(selected: &BTreeSet<String>, path: &str) -> Vec<String> {
    if selected.contains(path) && selected.len() > 1 {
        super::selection::dedupe_delete_paths(selected)
    } else {
        vec![path.to_owned()]
    }
}

/// `dest_dir` is a vault directory (`""` is the root). A drop is refused when
/// it would move a folder into itself, or when every item already lives there.
pub(crate) fn can_drop_into(paths: &[String], dest_dir: &str) -> bool {
    if paths.is_empty() {
        return false;
    }
    let dest = dest_dir.trim().trim_matches('/');
    if paths
        .iter()
        .any(|path| path == dest || is_prefix(path, dest))
    {
        return false;
    }
    paths.iter().any(|path| parent_dir(path) != dest)
}

pub(crate) fn parent_dir(path: &str) -> &str {
    path.rfind('/').map(|index| &path[..index]).unwrap_or("")
}

/// Highlight and, on release, accept a payload that may land in `dest_dir`.
pub(crate) fn take_drop(response: &egui::Response, dest_dir: &str) -> Option<Vec<String>> {
    let acceptable = response
        .dnd_hover_payload::<MovePayload>()
        .is_some_and(|payload| can_drop_into(&payload.paths, dest_dir));
    if !acceptable {
        return None;
    }
    response
        .dnd_release_payload::<MovePayload>()
        .map(|payload| payload.paths.clone())
}

pub(crate) fn drop_hot(response: &egui::Response, dest_dir: &str) -> bool {
    response
        .dnd_hover_payload::<MovePayload>()
        .is_some_and(|payload| can_drop_into(&payload.paths, dest_dir))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drag_of_a_selected_row_carries_the_whole_selection() {
        let mut selected = BTreeSet::new();
        selected.insert("a.txt".into());
        selected.insert("dir".into());
        selected.insert("dir/child.txt".into());
        let paths = paths_for_drag(&selected, "a.txt");
        assert_eq!(paths.len(), 2);
        assert!(paths.contains(&"a.txt".to_string()));
        assert!(paths.contains(&"dir".to_string()));
        assert!(!paths.iter().any(|path| path.contains('/')));
    }

    #[test]
    fn drag_of_an_unselected_row_carries_only_that_row() {
        let mut selected = BTreeSet::new();
        selected.insert("a.txt".into());
        let paths = paths_for_drag(&selected, "b.txt");
        assert_eq!(paths, vec!["b.txt".to_string()]);
    }

    #[test]
    fn drop_into_self_or_a_descendant_is_refused() {
        assert!(!can_drop_into(&["docs".into()], "docs"));
        assert!(!can_drop_into(&["docs".into()], "docs/specs"));
        assert!(can_drop_into(&["docs/a.txt".into()], "photos"));
        assert!(can_drop_into(&["docs/a.txt".into()], ""));
    }

    #[test]
    fn drop_is_a_no_op_when_everything_already_lives_there() {
        assert!(!can_drop_into(&["notes.txt".into()], ""));
        assert!(!can_drop_into(&["docs/a.txt".into()], "docs"));
        assert!(can_drop_into(
            &["docs/a.txt".into(), "notes.txt".into()],
            "docs"
        ));
    }
}
