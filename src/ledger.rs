//! The Ledger: Quad-Engine's document model.
//!
//! Every project asset (a Bezier path, a symbol instance, a timeline frame, an audio clip, a
//! layer) is an `EntityId`. Its actual data lives in one or more typed `ComponentStore<C>`s.
//! Mutating a component through `Ledger::write` captures a before/after snapshot as a `Delta`.
//!
//! Two undo mechanisms exist side by side, matching the two things a Flash-style editor
//! actually needs:
//!
//! - **Global undo** (`Ledger::undo` / `redo`) -- the main Ctrl+Z stack. Deltas are grouped into
//!   `Transaction`s so a compound edit (e.g. dragging 5 selected shapes) undoes as one step.
//! - **Per-asset undo** (`Ledger::undo_entity` / `redo_entity`) -- each entity keeps its own
//!   local delta log independent of the global stack, so e.g. scrubbing back one layer's paint
//!   history doesn't touch an unrelated timeline edit made in between. This is the ledger
//!   property Sailbrush's own model is built around.
//!
//! No external dependencies -- this crate needs to compile to both native (desktop pipeline)
//! and wasm32 (browser pipeline), so it only reaches for `std`.

use std::{
    any::{Any, TypeId},
    collections::{HashMap, VecDeque},
    fmt,
};

/// How many deltas each entity's local undo log keeps before evicting the oldest. Keeps
/// long-lived projects (which is to say, all of them) from growing memory unboundedly, at the
/// cost of a bound on how far back per-asset undo can reach. The global transaction log has its
/// own, separate cap (see `Ledger::MAX_TRANSACTIONS`).
const MAX_LOCAL_DELTAS_PER_ENTITY: usize = 256;

/// A generational handle to an asset. The generation guards against use-after-free: if an
/// entity is despawned and its slot reused, old `EntityId`s referring to the previous occupant
/// compare unequal to the new one and all lookups against it fail cleanly instead of aliasing.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct EntityId {
    index: u32,
    generation: u32,
}

impl fmt::Debug for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Entity({}v{})", self.index, self.generation)
    }
}

struct Slot {
    generation: u32,
    alive: bool,
}

#[derive(Default)]
struct EntityAllocator {
    slots: Vec<Slot>,
    free_list: Vec<u32>,
}

impl EntityAllocator {
    fn spawn(&mut self) -> EntityId {
        if let Some(index) = self.free_list.pop() {
            let slot = &mut self.slots[index as usize];
            slot.alive = true;
            EntityId { index, generation: slot.generation }
        } else {
            let index = self.slots.len() as u32;
            self.slots.push(Slot { generation: 0, alive: true });
            EntityId { index, generation: 0 }
        }
    }

    fn despawn(&mut self, id: EntityId) -> bool {
        let Some(slot) = self.slots.get_mut(id.index as usize) else { return false };
        if !slot.alive || slot.generation != id.generation {
            return false;
        }
        slot.alive = false;
        slot.generation = slot.generation.wrapping_add(1);
        self.free_list.push(id.index);
        true
    }

    fn is_alive(&self, id: EntityId) -> bool {
        self.slots.get(id.index as usize).is_some_and(|s| s.alive && s.generation == id.generation)
    }
}

/// Marker trait for anything that can live in a `ComponentStore`. Kept intentionally minimal --
/// `Clone` is required because every write snapshots the previous value for the undo log, so
/// components should be cheap to clone (Bezier control-point buffers, transforms, timeline
/// frame ranges, symbol references -- not e.g. a full rasterized bitmap; store a handle to that
/// instead).
pub trait Component: Clone + Send + Sync + 'static {}
impl<T: Clone + Send + Sync + 'static> Component for T {}

/// Typed storage for one component kind, keyed by entity. A real project will have one of these
/// per asset field that needs independent undo granularity -- e.g. `ComponentStore<BezierPath>`,
/// `ComponentStore<Transform>`, `ComponentStore<FrameRange>`.
pub struct ComponentStore<C: Component> {
    data: HashMap<EntityId, C>,
}

impl<C: Component> Default for ComponentStore<C> {
    fn default() -> Self {
        Self { data: HashMap::new() }
    }
}

impl<C: Component> ComponentStore<C> {
    pub fn get(&self, entity: EntityId) -> Option<&C> {
        self.data.get(&entity)
    }
}

/// Type-erased undo/redo action. `Ledger` stores these as trait objects so a single transaction
/// can mix deltas across unrelated component types (e.g. "move path" touches both `Transform`
/// and `BoundingBox`).
trait ErasedDelta: Send + Sync {
    fn entity(&self) -> EntityId;
    fn undo(&self, ledger: &mut Ledger);
    fn redo(&self, ledger: &mut Ledger);
    /// Object-safe clone, since `ErasedDelta` itself can't require `Self: Clone` and still be
    /// used as `Box<dyn ErasedDelta>` (see `Ledger::write`, which stores each delta in both the
    /// local per-entity log and the global transaction log).
    fn clone_box(&self) -> Box<dyn ErasedDelta>;
}

struct Delta<C: Component> {
    entity: EntityId,
    before: Option<C>,
    after: Option<C>,
}

impl<C: Component> ErasedDelta for Delta<C> {
    fn entity(&self) -> EntityId {
        self.entity
    }

    fn undo(&self, ledger: &mut Ledger) {
        ledger.restore::<C>(self.entity, self.before.clone());
    }

    fn redo(&self, ledger: &mut Ledger) {
        ledger.restore::<C>(self.entity, self.after.clone());
    }

    fn clone_box(&self) -> Box<dyn ErasedDelta> {
        Box::new(Delta { entity: self.entity, before: self.before.clone(), after: self.after.clone() })
    }
}

/// A batch of deltas that undo/redo together as one step on the *global* stack. Individual
/// entities can still be undone independently via `Ledger::undo_entity`, which ignores
/// transaction boundaries and just walks that entity's own local log.
#[derive(Default)]
struct Transaction {
    deltas: Vec<Box<dyn ErasedDelta>>,
}

/// The document. Owns every asset's component data plus both undo mechanisms.
#[derive(Default)]
pub struct Ledger {
    entities: EntityAllocator,
    stores: HashMap<TypeId, Box<dyn Any + Send + Sync>>,

    // Global undo: an ordered stack of transactions plus a cursor. Redoing after a fresh edit
    // truncates everything past the cursor, same as any standard editor undo stack.
    transactions: VecDeque<Transaction>,
    undo_cursor: usize,
    open_transaction: Option<Transaction>,

    // Per-asset undo: independent of the above. `local_cursor` points one past the delta that
    // would be undone next for that entity, mirroring `undo_cursor`'s semantics but scoped.
    local_logs: HashMap<EntityId, VecDeque<Box<dyn ErasedDelta>>>,
    local_cursors: HashMap<EntityId, usize>,
}

impl Ledger {
    const MAX_TRANSACTIONS: usize = 1000;

    pub fn new() -> Self {
        Self::default()
    }

    pub fn spawn(&mut self) -> EntityId {
        self.entities.spawn()
    }

    pub fn despawn(&mut self, entity: EntityId) {
        self.entities.despawn(entity);
        self.local_logs.remove(&entity);
        self.local_cursors.remove(&entity);
        // Deliberately not scrubbed from `stores` or `transactions` -- old deltas referencing a
        // despawned entity are harmless (their `restore` calls become no-ops via `is_alive`
        // checks below) and keeping them lets undo resurrect a despawned entity, which is
        // usually exactly what you want ("undo delete").
    }

    fn store<C: Component>(&mut self) -> &mut ComponentStore<C> {
        self.stores
            .entry(TypeId::of::<C>())
            .or_insert_with(|| Box::new(ComponentStore::<C>::default()))
            .downcast_mut::<ComponentStore<C>>()
            .expect("component store type mismatch -- this is a bug in Ledger, not caller code")
    }

    pub fn get<C: Component>(&self, entity: EntityId) -> Option<&C> {
        self.stores.get(&TypeId::of::<C>())?.downcast_ref::<ComponentStore<C>>()?.get(entity)
    }

    fn restore<C: Component>(&mut self, entity: EntityId, value: Option<C>) {
        match value {
            Some(v) => {
                self.store::<C>().data.insert(entity, v);
            }
            None => {
                self.store::<C>().data.remove(&entity);
            }
        }
    }

    /// Write (insert, update, or clear with `None`) a component, recording the change as a
    /// delta. If called inside `begin_transaction`/`commit_transaction`, the delta joins that
    /// transaction; otherwise it's wrapped in its own single-delta transaction automatically,
    /// so `write` is always safe to call standalone.
    pub fn write<C: Component>(&mut self, entity: EntityId, value: Option<C>) {
        if !self.entities.is_alive(entity) {
            return;
        }

        let before = self.get::<C>(entity).cloned();
        self.restore::<C>(entity, value.clone());

        let delta: Box<dyn ErasedDelta> = Box::new(Delta { entity, before, after: value });

        // Local (per-asset) log -- always recorded, transaction boundaries don't apply here.
        let local = self.local_logs.entry(entity).or_default();
        let cursor = *self.local_cursors.entry(entity).or_insert(0);
        local.truncate(cursor);
        local.push_back(delta.clone_box());
        while local.len() > MAX_LOCAL_DELTAS_PER_ENTITY {
            local.pop_front();
        }
        self.local_cursors.insert(entity, local.len());

        // Global log.
        match &mut self.open_transaction {
            Some(txn) => txn.deltas.push(delta),
            None => {
                self.transactions.truncate(self.undo_cursor);
                self.transactions.push_back(Transaction { deltas: vec![delta] });
                while self.transactions.len() > Self::MAX_TRANSACTIONS {
                    self.transactions.pop_front();
                }
                self.undo_cursor = self.transactions.len();
            }
        }
    }

    /// Groups every `write` between this call and `commit_transaction` into one global undo
    /// step (e.g. "drag 5 selected shapes" as a single Ctrl+Z, even though it's five writes to
    /// five different `Transform` components).
    pub fn begin_transaction(&mut self) {
        debug_assert!(self.open_transaction.is_none(), "nested transactions aren't supported -- commit the outer one first");
        self.open_transaction = Some(Transaction::default());
    }

    pub fn commit_transaction(&mut self) {
        let Some(txn) = self.open_transaction.take() else { return };
        if txn.deltas.is_empty() {
            return;
        }
        self.transactions.truncate(self.undo_cursor);
        self.transactions.push_back(txn);
        while self.transactions.len() > Self::MAX_TRANSACTIONS {
            self.transactions.pop_front();
        }
        self.undo_cursor = self.transactions.len();
    }

    /// Undo the most recent global transaction (which may span multiple entities/components).
    pub fn undo(&mut self) -> bool {
        if self.undo_cursor == 0 {
            return false;
        }
        self.undo_cursor -= 1;
        let deltas = std::mem::take(&mut self.transactions[self.undo_cursor].deltas);
        for delta in deltas.iter().rev() {
            delta.undo(self);
        }
        self.transactions[self.undo_cursor].deltas = deltas;
        true
    }

    pub fn redo(&mut self) -> bool {
        if self.undo_cursor >= self.transactions.len() {
            return false;
        }
        let deltas = std::mem::take(&mut self.transactions[self.undo_cursor].deltas);
        for delta in &deltas {
            delta.redo(self);
        }
        self.transactions[self.undo_cursor].deltas = deltas;
        self.undo_cursor += 1;
        true
    }

    /// Undo the most recent change to *this entity specifically*, ignoring what else happened
    /// globally in between. This is the "each asset keeps its own independent history" property.
    pub fn undo_entity(&mut self, entity: EntityId) -> bool {
        let cursor = self.local_cursors.get(&entity).copied().unwrap_or(0);
        if cursor == 0 {
            return false;
        }
        let Some(log) = self.local_logs.get(&entity) else { return false };
        let Some(delta) = log.get(cursor - 1) else { return false };
        delta.undo(self);
        self.local_cursors.insert(entity, cursor - 1);
        true
    }

    pub fn redo_entity(&mut self, entity: EntityId) -> bool {
        let cursor = self.local_cursors.get(&entity).copied().unwrap_or(0);
        let Some(log) = self.local_logs.get(&entity) else { return false };
        let Some(delta) = log.get(cursor) else { return false };
        delta.redo(self);
        self.local_cursors.insert(entity, cursor + 1);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq)]
    struct Position {
        x: f64,
        y: f64,
    }

    #[test]
    fn write_and_undo_roundtrip() {
        let mut ledger = Ledger::new();
        let e = ledger.spawn();

        ledger.write(e, Some(Position { x: 0.0, y: 0.0 }));
        ledger.write(e, Some(Position { x: 10.0, y: 0.0 }));

        assert_eq!(ledger.get::<Position>(e), Some(&Position { x: 10.0, y: 0.0 }));
        ledger.undo();
        assert_eq!(ledger.get::<Position>(e), Some(&Position { x: 0.0, y: 0.0 }));
        ledger.redo();
        assert_eq!(ledger.get::<Position>(e), Some(&Position { x: 10.0, y: 0.0 }));
    }

    #[test]
    fn per_entity_undo_is_independent_of_global_order() {
        let mut ledger = Ledger::new();
        let a = ledger.spawn();
        let b = ledger.spawn();

        ledger.write(a, Some(Position { x: 1.0, y: 0.0 }));
        ledger.write(b, Some(Position { x: 2.0, y: 0.0 }));
        ledger.write(a, Some(Position { x: 3.0, y: 0.0 }));

        // Undo only entity `a`'s most recent change; `b` must be untouched even though the
        // global stack interleaves the two entities' edits.
        ledger.undo_entity(a);
        assert_eq!(ledger.get::<Position>(a), Some(&Position { x: 1.0, y: 0.0 }));
        assert_eq!(ledger.get::<Position>(b), Some(&Position { x: 2.0, y: 0.0 }));
    }
}
