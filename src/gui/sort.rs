//! File-list ordering shared by the main browser and the tray panel.
//!
//! Directories stay above files. Within each group the active column decides
//! the order, and names compare in natural order (`file2` before `file10`).

use super::browser::BrowserNode;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum SortKey {
    #[default]
    Name,
    Size,
    Modified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Sort {
    pub(crate) key: SortKey,
    pub(crate) ascending: bool,
}

impl Default for Sort {
    fn default() -> Self {
        Self {
            key: SortKey::Name,
            ascending: true,
        }
    }
}

impl Sort {
    /// Click on a column: toggle direction if it is already active, otherwise
    /// switch to it. Name starts ascending; size and time start with the
    /// largest / newest first.
    pub(crate) fn toggle(&mut self, key: SortKey) {
        if self.key == key {
            self.ascending = !self.ascending;
        } else {
            self.key = key;
            self.ascending = key == SortKey::Name;
        }
    }
}

pub(crate) fn cmp_nodes(a: &BrowserNode, b: &BrowserNode, sort: Sort) -> Ordering {
    match (a.is_dir, b.is_dir) {
        (true, false) => return Ordering::Less,
        (false, true) => return Ordering::Greater,
        _ => {}
    }
    let ordering = match sort.key {
        SortKey::Name => natural_cmp(&a.name, &b.name),
        SortKey::Size => a
            .total_size()
            .cmp(&b.total_size())
            .then_with(|| natural_cmp(&a.name, &b.name)),
        SortKey::Modified => a
            .modified_at
            .cmp(&b.modified_at)
            .then_with(|| natural_cmp(&a.name, &b.name)),
    };
    if sort.ascending {
        ordering
    } else {
        ordering.reverse()
    }
}

/// Case-insensitive comparison that orders embedded numbers numerically
/// (`file2` < `file10`).
pub(crate) fn natural_cmp(a: &str, b: &str) -> Ordering {
    let mut ai = a.chars().peekable();
    let mut bi = b.chars().peekable();
    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(ca), Some(cb)) if ca.is_ascii_digit() && cb.is_ascii_digit() => {
                let mut na = 0u128;
                while let Some(c) = ai.peek().copied().filter(char::is_ascii_digit) {
                    na = na
                        .saturating_mul(10)
                        .saturating_add((c as u128) - u128::from(b'0'));
                    ai.next();
                }
                let mut nb = 0u128;
                while let Some(c) = bi.peek().copied().filter(char::is_ascii_digit) {
                    nb = nb
                        .saturating_mul(10)
                        .saturating_add((c as u128) - u128::from(b'0'));
                    bi.next();
                }
                if na != nb {
                    return na.cmp(&nb);
                }
            }
            (Some(ca), Some(cb)) => {
                let la = ca.to_lowercase().next().unwrap_or(ca);
                let lb = cb.to_lowercase().next().unwrap_or(cb);
                if la != lb {
                    return la.cmp(&lb);
                }
                ai.next();
                bi.next();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(name: &str, dir: bool, size: u64, modified: u64) -> BrowserNode {
        BrowserNode {
            name: name.to_owned(),
            path: name.to_owned(),
            is_dir: dir,
            size,
            modified_at: modified,
            children: Default::default(),
        }
    }

    #[test]
    fn natural_order_sorts_embedded_numbers() {
        assert_eq!(natural_cmp("file2", "file10"), Ordering::Less);
        assert_eq!(natural_cmp("File10", "file2"), Ordering::Greater);
        assert_eq!(natural_cmp("a", "a"), Ordering::Equal);
    }

    #[test]
    fn directories_stay_first_when_size_is_descending() {
        let mut rows = vec![
            node("notes.txt", false, 10, 1),
            node("photos", true, 0, 1),
            node("big.bin", false, 90, 1),
        ];
        let sort = Sort {
            key: SortKey::Size,
            ascending: false,
        };
        rows.sort_by(|a, b| cmp_nodes(a, b, sort));
        let names: Vec<_> = rows.iter().map(|row| row.name.as_str()).collect();
        assert_eq!(names, ["photos", "big.bin", "notes.txt"]);
    }

    #[test]
    fn modified_ascending_is_oldest_first_inside_the_group() {
        let mut rows = vec![node("new.txt", false, 1, 30), node("old.txt", false, 1, 10)];
        let sort = Sort {
            key: SortKey::Modified,
            ascending: true,
        };
        rows.sort_by(|a, b| cmp_nodes(a, b, sort));
        assert_eq!(rows[0].name, "old.txt");
    }
}
