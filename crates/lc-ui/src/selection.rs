//! Ordered multi-object selection shared by the viewport and object browser.
//!
//! The last id is the primary selection. Inspectors, mesh editing and other
//! single-object tools keep operating on it, while multi-object commands use
//! the full ordered set.

use lc_scene::{NodeId, Scene};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Selection {
    ids: Vec<NodeId>,
}

impl Selection {
    pub(super) fn primary(&self) -> Option<NodeId> {
        self.ids.last().copied()
    }

    pub(super) fn ids(&self) -> &[NodeId] {
        &self.ids
    }

    pub(super) fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    pub(super) fn len(&self) -> usize {
        self.ids.len()
    }

    pub(super) fn contains(&self, id: NodeId) -> bool {
        self.ids.contains(&id)
    }

    pub(super) fn clear(&mut self) {
        self.ids.clear();
    }

    pub(super) fn select_one(&mut self, id: NodeId) {
        self.ids.clear();
        self.ids.push(id);
    }

    /// Replace the selection, dropping duplicates and preserving order.
    pub(super) fn set(&mut self, ids: impl IntoIterator<Item = NodeId>) {
        self.ids.clear();
        for id in ids {
            self.add(id);
        }
    }

    pub(super) fn add(&mut self, id: NodeId) {
        if !self.contains(id) {
            self.ids.push(id);
        }
    }

    pub(super) fn remove(&mut self, id: NodeId) -> bool {
        if let Some(index) = self.ids.iter().position(|existing| *existing == id) {
            self.ids.remove(index);
            true
        } else {
            false
        }
    }

    /// Add `id`, or remove it when it is already selected. Adding makes it the
    /// primary selection, matching the last-clicked convention.
    pub(super) fn toggle(&mut self, id: NodeId) {
        if !self.remove(id) {
            self.ids.push(id);
        }
    }

    /// Drop ids that no longer exist, for example after undoing a delete.
    pub(super) fn retain_existing(&mut self, scene: &Scene) {
        self.ids.retain(|id| scene.get(*id).is_some());
    }
}

#[cfg(test)]
mod tests {
    use super::Selection;

    #[test]
    fn primary_is_the_last_added_id() {
        let mut selection = Selection::default();
        selection.set([4, 7, 9]);
        assert_eq!(selection.primary(), Some(9));
        assert_eq!(selection.ids(), &[4, 7, 9]);
    }

    #[test]
    fn toggle_adds_removes_and_keeps_duplicates_out() {
        let mut selection = Selection::default();
        selection.select_one(3);
        selection.toggle(5);
        selection.toggle(3);
        assert_eq!(selection.ids(), &[5]);
        selection.toggle(5);
        assert!(selection.is_empty());
        assert_eq!(selection.primary(), None);
    }

    #[test]
    fn replacing_a_selection_keeps_single_selection_behavior() {
        let mut selection = Selection::default();
        selection.set([1, 2, 3]);
        selection.select_one(8);
        assert_eq!(selection.len(), 1);
        assert_eq!(selection.primary(), Some(8));
        assert!(!selection.contains(1));
    }
}
