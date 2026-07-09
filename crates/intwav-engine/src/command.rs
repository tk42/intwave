//! Command-pattern editing (Q16). Every committed document mutation is a
//! [`Command`] with an inverse, so undo/redo is pure document manipulation —
//! zero audio cost, only possible because editing is non-destructive (Q9). The
//! undo/redo stacks are **session-volatile**; the document's `history` is a
//! separate **append-only provenance** log for archival audit.

use crate::document::{Document, Marker, OpChain, ProvenanceEntry, Region, SourceRef};
use crate::error::{EngineError, EngineResult, ErrorCode};

/// A reversible document mutation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    AddMarker(Marker),
    RemoveMarker(String),
    MoveMarker {
        id: String,
        frame: u64,
    },
    AddRegion(Region),
    RemoveRegion(String),
    SetRegionRange {
        id: String,
        start: u64,
        end: u64,
    },
    RenameRegion {
        id: String,
        title: String,
    },
    SetRegionOps {
        id: String,
        ops: OpChain,
    },
    /// Reorder regions to exactly this list of ids (a permutation of existing).
    ReorderRegions(Vec<String>),
}

fn not_found(kind: &str, id: &str) -> EngineError {
    EngineError::new(
        ErrorCode::InvalidParameter,
        format!("{kind} {id:?} not found"),
    )
}

impl Command {
    /// A short label for the provenance log.
    pub fn label(&self) -> &'static str {
        match self {
            Command::AddMarker(_) => "add-marker",
            Command::RemoveMarker(_) => "remove-marker",
            Command::MoveMarker { .. } => "move-marker",
            Command::AddRegion(_) => "add-region",
            Command::RemoveRegion(_) => "remove-region",
            Command::SetRegionRange { .. } => "set-region-range",
            Command::RenameRegion { .. } => "rename-region",
            Command::SetRegionOps { .. } => "set-region-ops",
            Command::ReorderRegions(_) => "reorder-regions",
        }
    }

    /// Apply this command to `doc`, returning the inverse command.
    fn apply(self, doc: &mut Document) -> EngineResult<Command> {
        match self {
            Command::AddMarker(m) => {
                let id = m.id.clone();
                doc.markers.push(m);
                Ok(Command::RemoveMarker(id))
            }
            Command::RemoveMarker(id) => {
                let pos = doc
                    .markers
                    .iter()
                    .position(|m| m.id == id)
                    .ok_or_else(|| not_found("marker", &id))?;
                let old = doc.markers.remove(pos);
                Ok(Command::AddMarker(old))
            }
            Command::MoveMarker { id, frame } => {
                let m = doc
                    .marker_mut(&id)
                    .ok_or_else(|| not_found("marker", &id))?;
                let old = m.frame;
                m.frame = frame;
                Ok(Command::MoveMarker { id, frame: old })
            }
            Command::AddRegion(r) => {
                let id = r.id.clone();
                doc.regions.push(r);
                Ok(Command::RemoveRegion(id))
            }
            Command::RemoveRegion(id) => {
                let pos = doc
                    .regions
                    .iter()
                    .position(|r| r.id == id)
                    .ok_or_else(|| not_found("region", &id))?;
                let old = doc.regions.remove(pos);
                Ok(Command::AddRegion(old))
            }
            Command::SetRegionRange { id, start, end } => {
                let r = doc
                    .region_mut(&id)
                    .ok_or_else(|| not_found("region", &id))?;
                let (os, oe) = (r.start_frame, r.end_frame);
                r.start_frame = start;
                r.end_frame = end;
                Ok(Command::SetRegionRange {
                    id,
                    start: os,
                    end: oe,
                })
            }
            Command::RenameRegion { id, title } => {
                let r = doc
                    .region_mut(&id)
                    .ok_or_else(|| not_found("region", &id))?;
                let old = std::mem::replace(&mut r.title, title);
                Ok(Command::RenameRegion { id, title: old })
            }
            Command::SetRegionOps { id, ops } => {
                let r = doc
                    .region_mut(&id)
                    .ok_or_else(|| not_found("region", &id))?;
                let old = std::mem::replace(&mut r.ops, ops);
                Ok(Command::SetRegionOps { id, ops: old })
            }
            Command::ReorderRegions(order) => {
                let old: Vec<String> = doc.regions.iter().map(|r| r.id.clone()).collect();
                if order.len() != old.len() || !is_permutation(&order, &old) {
                    return Err(EngineError::new(
                        ErrorCode::InvalidParameter,
                        "reorder must be a permutation of existing region ids",
                    ));
                }
                let mut reordered = Vec::with_capacity(order.len());
                for id in &order {
                    let pos = doc.regions.iter().position(|r| &r.id == id).unwrap();
                    reordered.push(doc.regions.remove(pos));
                }
                doc.regions = reordered;
                Ok(Command::ReorderRegions(old))
            }
        }
    }
}

fn is_permutation(a: &[String], b: &[String]) -> bool {
    let mut a2 = a.to_vec();
    let mut b2 = b.to_vec();
    a2.sort();
    b2.sort();
    a2 == b2
}

/// Owns a [`Document`] plus session-volatile undo/redo stacks.
pub struct Editor {
    doc: Document,
    undo: Vec<Command>,
    redo: Vec<Command>,
}

impl Editor {
    pub fn new(doc: Document) -> Self {
        Self {
            doc,
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    pub fn document(&self) -> &Document {
        &self.doc
    }

    pub fn into_document(self) -> Document {
        self.doc
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Register an immutable source. Adding a source is a structural,
    /// non-undoable operation (sources underpin regions) and is recorded in the
    /// provenance log.
    pub fn add_source(&mut self, src: SourceRef) {
        self.doc.history.push(ProvenanceEntry {
            action: "add-source".to_string(),
            detail: src.id.clone(),
        });
        self.doc.sources.push(src);
    }

    /// Apply a command, recording its inverse for undo and appending a
    /// provenance entry. Clears the redo stack (a new branch).
    pub fn apply(&mut self, cmd: Command) -> EngineResult<()> {
        let label = cmd.label().to_string();
        let inverse = cmd.apply(&mut self.doc)?;
        self.undo.push(inverse);
        self.redo.clear();
        self.doc.history.push(ProvenanceEntry {
            action: label,
            detail: String::new(),
        });
        Ok(())
    }

    /// Undo the last command. Returns false if nothing to undo.
    pub fn undo(&mut self) -> EngineResult<bool> {
        match self.undo.pop() {
            Some(inv) => {
                let redo_cmd = inv.apply(&mut self.doc)?;
                self.redo.push(redo_cmd);
                self.doc.history.push(ProvenanceEntry {
                    action: "undo".to_string(),
                    detail: String::new(),
                });
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// Redo the last undone command. Returns false if nothing to redo.
    pub fn redo(&mut self) -> EngineResult<bool> {
        match self.redo.pop() {
            Some(cmd) => {
                let undo_cmd = cmd.apply(&mut self.doc)?;
                self.undo.push(undo_cmd);
                self.doc.history.push(ProvenanceEntry {
                    action: "redo".to_string(),
                    detail: String::new(),
                });
                Ok(true)
            }
            None => Ok(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn region(id: &str, start: u64, end: u64) -> Region {
        Region {
            id: id.to_string(),
            source_id: "src".to_string(),
            start_frame: start,
            end_frame: end,
            title: String::new(),
            track_number: None,
            artist: None,
            album: None,
            ops: OpChain::default(),
        }
    }

    #[test]
    fn add_then_undo_redo() {
        let mut ed = Editor::new(Document::new());
        ed.apply(Command::AddRegion(region("a", 0, 100))).unwrap();
        assert_eq!(ed.document().regions.len(), 1);
        assert!(ed.undo().unwrap());
        assert_eq!(ed.document().regions.len(), 0);
        assert!(ed.redo().unwrap());
        assert_eq!(ed.document().regions.len(), 1);
    }

    #[test]
    fn move_marker_inverts() {
        let mut ed = Editor::new(Document::new());
        ed.apply(Command::AddMarker(Marker {
            id: "m".into(),
            frame: 10,
            label: String::new(),
        }))
        .unwrap();
        ed.apply(Command::MoveMarker {
            id: "m".into(),
            frame: 50,
        })
        .unwrap();
        assert_eq!(ed.document().markers[0].frame, 50);
        ed.undo().unwrap();
        assert_eq!(ed.document().markers[0].frame, 10);
    }

    #[test]
    fn reorder_regions_roundtrip() {
        let mut ed = Editor::new(Document::new());
        for id in ["a", "b", "c"] {
            ed.apply(Command::AddRegion(region(id, 0, 1))).unwrap();
        }
        ed.apply(Command::ReorderRegions(vec![
            "c".into(),
            "a".into(),
            "b".into(),
        ]))
        .unwrap();
        let ids: Vec<_> = ed.document().regions.iter().map(|r| r.id.clone()).collect();
        assert_eq!(ids, vec!["c", "a", "b"]);
        ed.undo().unwrap();
        let ids: Vec<_> = ed.document().regions.iter().map(|r| r.id.clone()).collect();
        assert_eq!(ids, vec!["a", "b", "c"]);
    }

    #[test]
    fn redo_cleared_on_new_apply() {
        let mut ed = Editor::new(Document::new());
        ed.apply(Command::AddRegion(region("a", 0, 1))).unwrap();
        ed.undo().unwrap();
        assert!(ed.can_redo());
        ed.apply(Command::AddRegion(region("b", 0, 1))).unwrap();
        assert!(!ed.can_redo()); // new branch cleared redo
    }

    #[test]
    fn provenance_is_append_only() {
        let mut ed = Editor::new(Document::new());
        ed.apply(Command::AddRegion(region("a", 0, 1))).unwrap();
        ed.undo().unwrap();
        // apply + undo both recorded; nothing removed.
        assert_eq!(ed.document().history.len(), 2);
        assert_eq!(ed.document().history[0].action, "add-region");
        assert_eq!(ed.document().history[1].action, "undo");
    }
}
