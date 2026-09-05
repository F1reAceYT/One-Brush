//! Demo: draw a Bezier path into the Ledger, then walk both undo stacks.
//!
//! Shows the two mechanisms the ledger exposes:
//! - global undo/redo (the Ctrl+Z stack, transactions undo as one step), and
//! - per-asset undo/redo (`undo_entity` / `redo_entity`), which scrubs a single
//!   entity's own local history without touching anything else.

use quad_engine_core::Ledger;

/// One vector path in the document. Cheap to clone (just control points), so it can live in a
/// `ComponentStore` and be snapshotted on every `write` for the undo logs.
#[derive(Clone, Debug, PartialEq)]
struct BezierPath {
    points: Vec<(f64, f64)>,
    closed: bool,
}

fn main() {
    let mut ledger = Ledger::new();
    let shape = ledger.spawn();

    let path = |pts: Vec<(f64, f64)>, closed: bool| BezierPath { points: pts, closed };

    // Seed a path on the entity, then group a small edit as a single global undo step.
    ledger.write(shape, Some(path(vec![(0.0, 0.0), (1.0, 1.0)], false)));
    ledger.begin_transaction();
    ledger.write(shape, Some(path(vec![(0.0, 0.0), (1.0, 1.0), (2.0, 0.0)], false)));
    ledger.write(shape, Some(path(vec![(0.0, 0.0), (1.0, 1.0), (2.0, 0.0)], true)));
    ledger.commit_transaction();

    println!("after edit:    {:?}", ledger.get::<BezierPath>(shape).unwrap());

    // Global undo rewinds the whole transaction in one step (both writes).
    ledger.undo();
    println!("global undo:   {:?}", ledger.get::<BezierPath>(shape).unwrap());

    // Per-asset undo keeps moving just this entity's history back.
    ledger.undo_entity(shape);
    println!("undo_entity:   {:?}", ledger.get::<BezierPath>(shape));

    // Redo walks forward again, still scoped to the entity.
    ledger.redo_entity(shape);
    println!("redo_entity:   {:?}", ledger.get::<BezierPath>(shape).unwrap());

    // Global redo brings the whole transaction back.
    ledger.redo();
    println!("global redo:   {:?}", ledger.get::<BezierPath>(shape).unwrap());
}
