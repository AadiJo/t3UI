//! Sidebar multi-selection (`web/threadSelectionStore.ts`). In memory only.

use std::collections::HashSet;

use crate::refs::ThreadRef;

/// Selected thread rows plus the anchor shift-click ranges start from.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThreadSelection {
    selected: HashSet<ThreadRef>,
    anchor: Option<ThreadRef>,
}

impl ThreadSelection {
    pub fn is_empty(&self) -> bool {
        self.selected.is_empty()
    }

    pub fn len(&self) -> usize {
        self.selected.len()
    }

    pub fn contains(&self, thread: &ThreadRef) -> bool {
        self.selected.contains(thread)
    }

    /// Selected threads in `order` (sidebar order), for bulk actions.
    pub fn in_order<'a>(
        &'a self,
        order: &'a [ThreadRef],
    ) -> impl Iterator<Item = &'a ThreadRef> + 'a {
        order.iter().filter(|thread| self.selected.contains(thread))
    }

    /// Mod-click: toggles `thread`; it becomes the anchor if it was added.
    pub fn toggle(&mut self, thread: &ThreadRef) {
        if self.selected.remove(thread) {
            return;
        }
        self.selected.insert(thread.clone());
        self.anchor = Some(thread.clone());
    }

    /// Shift-click: adds every thread between the anchor and `thread` in `ordered` (that
    /// project's threads). Without a usable anchor it just adds `thread` and anchors there.
    pub fn select_range(&mut self, thread: &ThreadRef, ordered: &[ThreadRef]) {
        let indexes = self.anchor.as_ref().and_then(|anchor| {
            let start = ordered.iter().position(|candidate| candidate == anchor)?;
            let end = ordered.iter().position(|candidate| candidate == thread)?;
            Some((start.min(end), start.max(end)))
        });
        match indexes {
            Some((start, end)) => self.selected.extend(ordered[start..=end].iter().cloned()),
            None => {
                self.selected.insert(thread.clone());
                self.anchor = Some(thread.clone());
            }
        }
    }

    /// Clears the selection and the anchor. Returns whether anything changed.
    pub fn clear(&mut self) -> bool {
        let changed = !self.selected.is_empty() || self.anchor.is_some();
        self.selected.clear();
        self.anchor = None;
        changed
    }

    /// Sets the anchor without selecting (plain-click navigation).
    pub fn set_anchor(&mut self, thread: &ThreadRef) {
        self.anchor = Some(thread.clone());
    }

    /// Drops deleted threads; the anchor goes too if it was one of them.
    pub fn remove(&mut self, threads: &[ThreadRef]) {
        for thread in threads {
            self.selected.remove(thread);
        }
        if self
            .anchor
            .as_ref()
            .is_some_and(|anchor| threads.contains(anchor))
        {
            self.anchor = None;
        }
    }
}

#[cfg(test)]
mod tests {
    //! Failure modes: toggling off moving the anchor, ranges ignoring the anchor's position
    //! (up vs down), ranges across projects, and clear leaving a stale anchor.
    use super::*;

    fn refs(ids: &[&str]) -> Vec<ThreadRef> {
        ids.iter()
            .map(|id| ThreadRef::new("e".into(), (*id).into()))
            .collect()
    }

    #[test]
    fn toggle_and_range() {
        let order = refs(&["a", "b", "c", "d"]);
        let mut selection = ThreadSelection::default();
        selection.toggle(&order[2]);
        selection.select_range(&order[0], &order);
        let picked: Vec<_> = selection.in_order(&order).cloned().collect();
        assert_eq!(picked, order[..3].to_vec());

        // Toggling off keeps the anchor at c.
        selection.toggle(&order[1]);
        assert!(!selection.contains(&order[1]));
        selection.select_range(&order[3], &order);
        assert!(selection.contains(&order[2]) && selection.contains(&order[3]));
        assert!(!selection.contains(&order[1]));
    }

    #[test]
    fn range_without_anchor_in_list_adds_one() {
        let order = refs(&["a", "b"]);
        let other = refs(&["x"]);
        let mut selection = ThreadSelection::default();
        selection.set_anchor(&other[0]);
        selection.select_range(&order[1], &order);
        assert_eq!(selection.len(), 1);
        // The clicked thread is now the anchor.
        selection.select_range(&order[0], &order);
        assert_eq!(selection.len(), 2);
        assert!(selection.clear());
        assert!(!selection.clear());
    }

    #[test]
    fn removing_the_anchor_clears_it() {
        let order = refs(&["a", "b", "c"]);
        let mut selection = ThreadSelection::default();
        selection.toggle(&order[0]);
        selection.remove(&order[..1]);
        assert!(selection.is_empty());
        selection.select_range(&order[2], &order);
        assert_eq!(selection.len(), 1);
    }
}
