//! FLA import: entry point.
//!
//! Scope, per the decision to keep this bounded: reads CS4+ format `.fla` files (a zip
//! containing `DOMDocument.xml` + `LIBRARY/*.xml`). Does not read pre-CS4 binary FLA. Does not
//! write/export. ActionScript is round-tripped as opaque text (see `Component`s below), not
//! interpreted -- this crate has no AS runtime and isn't trying to be one.

mod fla_dom;
pub mod edge;
pub mod geometry;

use std::{
    collections::HashMap,
    fs::File,
    io::{self, Read},
    path::Path,
};

use quick_xml::de::{from_str, DeError};

use crate::ledger::{EntityId, Ledger};

#[derive(Debug)]
pub enum FlaImportError {
    Io(io::Error),
    Zip(zip::result::ZipError),
    MissingDomDocument,
    Xml { file: String, source: DeError },
}

impl From<io::Error> for FlaImportError {
    fn from(e: io::Error) -> Self {
        FlaImportError::Io(e)
    }
}

impl From<zip::result::ZipError> for FlaImportError {
    fn from(e: zip::result::ZipError) -> Self {
        FlaImportError::Zip(e)
    }
}

/// Components a shape entity carries after import. `raw_edges` is kept alongside the parsed
/// `path` (once `edge.rs`'s tokenizer exists) deliberately -- for the first shipping version,
/// re-exporting the untouched original string for any shape we didn't successfully reparse is a
/// much safer fallback than dropping geometry the parser choked on.
#[derive(Clone, Debug)]
pub struct ShapePath {
    pub raw_edges: String,
    // pub segments: Vec<geometry::CubicSegment>, // populated once edge.rs's tokenizer lands
}

/// A symbol (MovieClip/Graphic/Button) imported from `LIBRARY/*.xml`.
#[derive(Clone, Debug)]
pub struct Symbol {
    pub name: String,
    pub symbol_type: Option<String>,
}

/// A placed instance of a symbol on some parent timeline.
#[derive(Clone, Debug)]
pub struct SymbolInstance {
    pub library_item_name: String,
    pub transform: geometry::Point, // translation only for now; full 2x3 matrix is straightforward to add once ShapePath's tokenizer is real and there's an actual transform pipeline to feed
}

pub struct ImportResult {
    pub ledger: Ledger,
    /// Library symbol name -> the entity representing that symbol definition, so callers can
    /// resolve `DomSymbolInstance::library_item_name` references after the fact.
    pub symbols_by_name: HashMap<String, EntityId>,
}

pub fn import_fla(path: impl AsRef<Path>) -> Result<ImportResult, FlaImportError> {
    let file = File::open(path)?;
    let mut archive = zip::ZipArchive::new(file)?;

    let dom_document: fla_dom::DomDocument = read_and_parse_xml(&mut archive, "DOMDocument.xml")?;

    let mut ledger = Ledger::new();
    let mut symbols_by_name = HashMap::new();

    // Pass 1: library symbols first, so main-timeline instances can resolve against them
    // regardless of which order the zip happens to list entries in.
    let library_files: Vec<String> = archive
        .file_names()
        .filter(|n| n.starts_with("LIBRARY/") && n.ends_with(".xml"))
        .map(str::to_owned)
        .collect();

    for name in library_files {
        let symbol_item: fla_dom::DomSymbolItem = read_and_parse_xml(&mut archive, &name)?;
        let entity = ledger.spawn();
        ledger.write(entity, Some(Symbol { name: symbol_item.name.clone(), symbol_type: symbol_item.symbol_type.clone() }));
        populate_timeline(&mut ledger, entity, &symbol_item.timeline);
        symbols_by_name.insert(symbol_item.name, entity);
    }

    // Pass 2: the main document's own timeline(s).
    for timeline in &dom_document.timelines.timeline {
        let root_entity = ledger.spawn();
        populate_timeline(&mut ledger, root_entity, timeline);
    }

    Ok(ImportResult { ledger, symbols_by_name })
}

fn populate_timeline(ledger: &mut Ledger, _timeline_owner: EntityId, timeline: &fla_dom::DomTimeline) {
    for layer in &timeline.layers.layer {
        for frame in &layer.frames.frame {
            let Some(elements) = &frame.elements else { continue };

            for shape in &elements.shapes {
                let entity = ledger.spawn();
                // A DOMShape can carry multiple <Edge> records (e.g. separate fill and stroke
                // edges referencing the same outline); concatenating here is a placeholder --
                // once edge.rs is real, each Edge likely wants to become its own path with its
                // own fill/stroke style rather than being flattened into one string.
                let raw_edges = shape.edges.edge.iter().map(|e| e.edges.as_str()).collect::<Vec<_>>().join(" ");
                ledger.write(entity, Some(ShapePath { raw_edges }));
            }

            for instance in &elements.symbol_instances {
                let entity = ledger.spawn();
                let (tx, ty) = instance.matrix.as_ref().map(|m| (m.inner.tx, m.inner.ty)).unwrap_or((0.0, 0.0));
                ledger.write(
                    entity,
                    Some(SymbolInstance { library_item_name: instance.library_item_name.clone(), transform: geometry::Point { x: tx, y: ty } }),
                );
            }
        }
    }
}

fn read_and_parse_xml<T: serde::de::DeserializeOwned>(archive: &mut zip::ZipArchive<File>, name: &str) -> Result<T, FlaImportError> {
    let mut entry = archive.by_name(name).map_err(|_| FlaImportError::MissingDomDocument)?;
    let mut contents = String::new();
    entry.read_to_string(&mut contents)?;
    from_str(&contents).map_err(|e| FlaImportError::Xml { file: name.to_owned(), source: e })
}
