//! Typed model of the XML Adobe Animate writes inside an FLA's zip container.
//!
//! Confidence note, read before trusting field names here: the *existence* and general shape of
//! `DOMDocument`, `DOMTimeline`, `DOMLayer`, `DOMFrame`, `DOMShape`, `DOMSymbolItem`, and
//! `DOMSymbolInstance` as top-level XML elements is well attested across independent
//! community sources (JSFL scripting docs reference this exact object model, and it's been
//! stable since the format's CS4 introduction). What is *not* independently verified here is
//! every attribute name and nesting detail below -- some fields are filled in from reasonably
//! confident recollection, not a source I could cite. Treat struct fields marked `// unverified`
//! as needing a real sample file to confirm, same caveat as `edge.rs`'s tokenizer. `#[serde(other)]`-
//! style leniency is used throughout so an unrecognized attribute doesn't hard-fail the whole
//! parse -- better to import a shape with a missing fill color than to reject the file outright.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename = "DOMDocument")]
pub struct DomDocument {
    #[serde(rename = "@width")]
    pub width: f64,
    #[serde(rename = "@height")]
    pub height: f64,
    #[serde(rename = "@frameRate")]
    pub frame_rate: f64,
    pub timelines: Timelines,
}

#[derive(Debug, Deserialize)]
pub struct Timelines {
    #[serde(rename = "DOMTimeline")]
    pub timeline: Vec<DomTimeline>,
}

#[derive(Debug, Deserialize)]
pub struct DomTimeline {
    #[serde(rename = "@name")]
    pub name: String,
    pub layers: Layers,
}

#[derive(Debug, Deserialize)]
pub struct Layers {
    #[serde(rename = "DOMLayer")]
    pub layer: Vec<DomLayer>,
}

#[derive(Debug, Deserialize)]
pub struct DomLayer {
    #[serde(rename = "@name")]
    pub name: String,
    // unverified: locked/visible attribute names -- likely "locked"/"visible" as bool-ish
    // strings, or possibly encoded differently (Animate has historically been inconsistent
    // about bool-as-string vs bool-as-"1"/"0" across attributes).
    #[serde(rename = "@locked", default)]
    pub locked: bool,
    pub frames: Frames,
}

#[derive(Debug, Deserialize)]
pub struct Frames {
    #[serde(rename = "DOMFrame")]
    pub frame: Vec<DomFrame>,
}

#[derive(Debug, Deserialize)]
pub struct DomFrame {
    #[serde(rename = "@index")]
    pub index: u32,
    #[serde(rename = "@duration", default = "one")]
    pub duration: u32,
    // unverified: tween-type attribute name/values ("motion", "shape", "none" or similar enum).
    #[serde(rename = "@tweenType", default)]
    pub tween_type: Option<String>,
    pub elements: Option<Elements>,
}

fn one() -> u32 {
    1
}

/// A frame's contents: a mix of shapes and symbol instances. `quick-xml`'s serde layer doesn't
/// love "any of these element kinds in original document order" out of the box -- for a first
/// pass this flattens shapes and instances into separate `Vec`s (losing relative z-order between
/// the two *kinds*, though order within each kind is preserved), which is wrong for layers that
/// mix raw shapes and symbol instances at the same depth. Flagging rather than silently
/// shipping it: needs a proper `#[serde(flatten)]`-friendly enum once verified against a real
/// multi-element frame.
#[derive(Debug, Deserialize, Default)]
pub struct Elements {
    #[serde(rename = "DOMShape", default)]
    pub shapes: Vec<DomShape>,
    #[serde(rename = "DOMSymbolInstance", default)]
    pub symbol_instances: Vec<DomSymbolInstance>,
}

#[derive(Debug, Deserialize)]
pub struct DomShape {
    pub edges: Edges,
    // fills/strokes deliberately omitted from this pass -- geometry (edges) is the
    // higher-value, harder problem; style attributes are comparatively mechanical to add once
    // the shape/path pipeline itself is proven against real files.
}

#[derive(Debug, Deserialize)]
pub struct Edges {
    #[serde(rename = "Edge", default)]
    pub edge: Vec<Edge>,
}

#[derive(Debug, Deserialize)]
pub struct Edge {
    /// The raw path mini-language string -- see `super::edge` for the (currently unverified)
    /// tokenizer.
    #[serde(rename = "@edges")]
    pub edges: String,
}

#[derive(Debug, Deserialize)]
pub struct DomSymbolInstance {
    #[serde(rename = "@libraryItemName")]
    pub library_item_name: String,
    pub matrix: Option<Matrix>,
}

#[derive(Debug, Deserialize)]
pub struct Matrix {
    #[serde(rename = "Matrix")]
    pub inner: MatrixValues,
}

#[derive(Debug, Deserialize)]
pub struct MatrixValues {
    #[serde(rename = "@a", default = "identity_scale")]
    pub a: f64,
    #[serde(rename = "@d", default = "identity_scale")]
    pub d: f64,
    #[serde(rename = "@b", default)]
    pub b: f64,
    #[serde(rename = "@c", default)]
    pub c: f64,
    #[serde(rename = "@tx", default)]
    pub tx: f64,
    #[serde(rename = "@ty", default)]
    pub ty: f64,
}

fn identity_scale() -> f64 {
    1.0
}

/// One `LIBRARY/*.xml` file -- a single symbol's own timeline, structurally similar to the main
/// document's but rooted differently.
#[derive(Debug, Deserialize)]
#[serde(rename = "DOMSymbolItem")]
pub struct DomSymbolItem {
    #[serde(rename = "@name")]
    pub name: String,
    // unverified: symbolType attribute values -- "graphic" / "movie clip" / "button" is the
    // expected set based on Animate's own symbol-type UI, but not confirmed against raw XML.
    #[serde(rename = "@symbolType", default)]
    pub symbol_type: Option<String>,
    pub timeline: DomTimeline,
}
