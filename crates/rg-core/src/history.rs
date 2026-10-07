//! Snapshot-based undo. Meshes are reference counted, so a snapshot of a
//! scene costs roughly its node table, not its geometry.

use crate::scene::Scene;

#[derive(Default)]
pub struct History {
    undo: Vec<(String, Scene)>,
    redo: Vec<(String, Scene)>,
    pub limit: usize,
}

impl History {
    pub fn new() -> Self {
        Self { undo: Vec::new(), redo: Vec::new(), limit: 200 }
    }

    /// Record the state *before* a change.
    pub fn push(&mut self, label: impl Into<String>, before: &Scene) {
        self.undo.push((label.into(), before.clone()));
        if self.undo.len() > self.limit.max(1) {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub fn undo(&mut self, current: &mut Scene) -> Option<String> {
        let (label, prev) = self.undo.pop()?;
        let now = std::mem::replace(current, prev);
        self.redo.push((label.clone(), now));
        current.touch();
        Some(label)
    }

    pub fn redo(&mut self, current: &mut Scene) -> Option<String> {
        let (label, next) = self.redo.pop()?;
        let now = std::mem::replace(current, next);
        self.undo.push((label.clone(), now));
        current.touch();
        Some(label)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn undo_labels(&self) -> Vec<&str> {
        self.undo.iter().rev().map(|(l, _)| l.as_str()).collect()
    }
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::NodeKind;

    #[test]
    fn undo_redo() {
        let mut s = Scene::new();
        let mut h = History::new();
        h.push("create", &s);
        s.add_node("locator", NodeKind::Locator, None);
        assert_eq!(s.nodes.len(), 1);
        h.undo(&mut s);
        assert_eq!(s.nodes.len(), 0);
        h.redo(&mut s);
        assert_eq!(s.nodes.len(), 1);
    }
}
