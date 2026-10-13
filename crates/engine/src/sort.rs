//! Sorting for the content panel: folders first, then by the chosen key.

use assetcraft_fs::{Entry, EntryKind};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

/// What the content panel sorts by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortKey {
    /// File name, natural order (`shot_2` before `shot_10`), case-insensitive.
    #[default]
    Name,
    /// Modification date.
    Date,
    /// File size.
    Size,
    /// Extension, then name.
    Type,
}

impl SortKey {
    /// Parse a command parameter.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "name" => Self::Name,
            "date" => Self::Date,
            "size" => Self::Size,
            "type" => Self::Type,
            _ => return None,
        })
    }
}

/// Sort in place. Folders always come before files regardless of direction; ties fall back to
/// natural name order so the result is stable and predictable.
pub fn sort_entries(entries: &mut [Entry], key: SortKey, ascending: bool) {
    entries.sort_by(|a, b| {
        let folders_first = (b.kind == EntryKind::Dir).cmp(&(a.kind == EntryKind::Dir));
        let primary = match key {
            SortKey::Name => Ordering::Equal,
            SortKey::Date => a.modified.cmp(&b.modified),
            SortKey::Size => a.size.cmp(&b.size),
            SortKey::Type => a.extension().cmp(&b.extension()),
        };
        let primary = if ascending { primary } else { primary.reverse() };
        let by_name = natural_cmp(&a.name, &b.name);
        let by_name = if ascending || key != SortKey::Name { by_name } else { by_name.reverse() };
        folders_first.then(primary).then(by_name)
    });
}

/// Natural, case-insensitive comparison: runs of digits compare by value.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let na = take_number(&mut a);
                let nb = take_number(&mut b);
                // Compare by value: strip leading zeros, then length, then digits.
                let (ta, tb) = (na.trim_start_matches('0'), nb.trim_start_matches('0'));
                let ord = ta.len().cmp(&tb.len()).then_with(|| ta.cmp(tb)).then_with(|| na.len().cmp(&nb.len()));
                if ord != Ordering::Equal {
                    return ord;
                }
            }
            (Some(x), Some(y)) => {
                let ord = x.to_lowercase().cmp(y.to_lowercase());
                if ord != Ordering::Equal {
                    return ord;
                }
                a.next();
                b.next();
            }
        }
    }
}

fn take_number(it: &mut std::iter::Peekable<std::str::Chars<'_>>) -> String {
    let mut s = String::new();
    while let Some(c) = it.peek().copied().filter(char::is_ascii_digit) {
        s.push(c);
        it.next();
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    fn e(name: &str, kind: EntryKind, size: u64, secs: u64) -> Entry {
        Entry {
            name: name.into(),
            path: format!("/x/{name}").into(),
            kind,
            size,
            modified: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(secs)),
            hidden: false,
        }
    }

    fn names(v: &[Entry]) -> Vec<&str> {
        v.iter().map(|e| e.name.as_str()).collect()
    }

    #[test]
    fn natural_order() {
        let mut v = ["shot_10.png", "Shot_2.png", "shot_1.png", "reaper_concept_v03.png", "reaper_concept_v01.png"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(v, ["reaper_concept_v01.png", "reaper_concept_v03.png", "shot_1.png", "Shot_2.png", "shot_10.png"]);
        assert_eq!(natural_cmp("a01", "a1"), Ordering::Greater); // same value: shorter first, stable
        assert_eq!(natural_cmp("abc", "ABC"), Ordering::Equal);
    }

    #[test]
    fn folders_first_in_both_directions() {
        let mut v =
            vec![e("b.png", EntryKind::File, 5, 3), e("z", EntryKind::Dir, 0, 1), e("a.png", EntryKind::File, 9, 2)];
        sort_entries(&mut v, SortKey::Name, true);
        assert_eq!(names(&v), ["z", "a.png", "b.png"]);
        sort_entries(&mut v, SortKey::Name, false);
        assert_eq!(names(&v), ["z", "b.png", "a.png"]);
    }

    #[test]
    fn by_date_size_and_type() {
        let mut v = vec![
            e("a.png", EntryKind::File, 9, 30),
            e("b.jpg", EntryKind::File, 1, 10),
            e("c.bmp", EntryKind::File, 5, 20),
        ];
        sort_entries(&mut v, SortKey::Date, true);
        assert_eq!(names(&v), ["b.jpg", "c.bmp", "a.png"]);
        sort_entries(&mut v, SortKey::Size, false);
        assert_eq!(names(&v), ["a.png", "c.bmp", "b.jpg"]);
        sort_entries(&mut v, SortKey::Type, true);
        assert_eq!(names(&v), ["c.bmp", "b.jpg", "a.png"]);
    }
}
