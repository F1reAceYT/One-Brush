//! Tokenizer/parser for Adobe Animate's `cubics=` edge string format.
//!
//! The format (decoded from real CS6-era FLA files):
//!
//! ```text
//! cubics="!8190 5402(;9125,5402 9317,5102 9735,4365q8190 5402Q8537 5402...);"
//!        ^^^^^^^^ ^ ^^^^^^^^^^^^^^^^^^^^^^^^^^^^ ^^^^^^^^^^^^^^^^^^^^^^^^^^
//!        |        | |                              |
//!        |        | |                              +-- quadratic approximation fallback
//!        |        | |                                 (alternating q/Q = anchor/control,
//!        |        | |                                  discard for lossless import)
//!        |        | +-- CUBIC curve data (what we want):
//!        |        |     comma-separated coordinate pairs:
//!        |        |     each cubic = 3 pairs = 6 numbers (ctrl1, ctrl2, end)
//!        |        |     multiple cubics space-separated
//!        |        +-- optional extra coordinate pair (e.g. "8360,5507")
//!        |           appears in some edges between '(' and ';'
//!        +-- moveto starting point: "!x y"
//! ```
//!
//! DECISION: We parse and KEEP only the cubic curve data (the lossless representation).
//! The trailing quadratic approximation (q/Q sequence) is a backward-compat fallback for
//! consumers that only understand quadratic edges (the original SWF SHAPE record format).
//! Discarding it is correct because: (1) it's a lossy re-approximation of the true cubics,
//! (2) our pipeline standardizes on cubics anyway (see `geometry::CubicSegment`), and
//! (3) keeping both would duplicate every curve.
//!
//! The "extra coordinate pair" between '(' and ';' (e.g. "8360,5507" in
//! "!8360 5507(8360,5507;...)") appears to be a redundant restatement of the moveto
//! point, possibly a corner/anchor flag in the editor. We detect and skip it.

use crate::import::geometry::{Point, CubicSegment};

#[derive(Clone, Debug, PartialEq)]
pub struct ParsedEdge {
    pub start: Point,
    pub cubics: Vec<CubicSegment>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EdgeParseError {
    Empty,
    MissingMoveto(String),
    InvalidCoordinate(String),
    UnexpectedEof,
    MalformedCubic(usize),
}

impl core::fmt::Display for EdgeParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            EdgeParseError::Empty => write!(f, "empty string"),
            EdgeParseError::MissingMoveto(s) => write!(f, "missing moveto '!': {s}"),
            EdgeParseError::InvalidCoordinate(s) => write!(f, "invalid coordinate: {s}"),
            EdgeParseError::UnexpectedEof => write!(f, "unexpected end of input"),
            EdgeParseError::MalformedCubic(n) => write!(f, "malformed cubic data: expected 6 numbers per cubic, got {n}"),
        }
    }
}

impl std::error::Error for EdgeParseError {}

fn parse_f64(s: &str) -> Result<f64, EdgeParseError> {
    s.parse().map_err(|_| EdgeParseError::InvalidCoordinate(s.to_owned()))
}

/// Parse a space-separated list of coordinate pairs like "9125,5402 9317,5102 9735,4365"
/// Returns Vec of (x, y) pairs.
fn parse_coord_pairs(s: &str) -> Result<Vec<(f64, f64)>, EdgeParseError> {
    let mut pairs = Vec::new();
    for token in s.split_whitespace() {
        if token.is_empty() {
            continue;
        }
        let parts: Vec<&str> = token.split(',').collect();
        if parts.len() != 2 {
            return Err(EdgeParseError::InvalidCoordinate(token.to_owned()));
        }
        let x = parse_f64(parts[0])?;
        let y = parse_f64(parts[1])?;
        pairs.push((x, y));
    }
    Ok(pairs)
}

/// Parse the quadratic-approximation tail (alternating q/Q) to find where cubics end.
/// Returns the index in the string where the quadratic tail begins (at first 'q' or 'Q'),
/// or the string length if none.
fn find_quadratic_tail_start(s: &str) -> usize {
    // The quadratic tail starts with lowercase 'q' (anchor) or uppercase 'Q' (control)
    // It follows immediately after the last cubic's endpoint.
    // Find the first 'q' or 'Q' that is NOT part of a coordinate (coordinates use ',' and space).
    s.find(|c: char| c == 'q' || c == 'Q').unwrap_or(s.len())
}

pub fn parse_cubics_edge(s: &str) -> Result<ParsedEdge, EdgeParseError> {
    let s = s.trim();
    if s.is_empty() {
        return Err(EdgeParseError::Empty);
    }

    // Must start with '!'
    let Some(rest) = s.strip_prefix('!') else {
        return Err(EdgeParseError::MissingMoveto(s.to_owned()));
    };

    // Parse moveto: "x y(" or "x y(x,y;"
    let moveto_end = rest.find(|c: char| c == '(').ok_or(EdgeParseError::UnexpectedEof)?;
    let moveto_str = &rest[..moveto_end];
    let moveto_parts: Vec<&str> = moveto_str.split_whitespace().collect();
    if moveto_parts.len() != 2 {
        return Err(EdgeParseError::InvalidCoordinate(moveto_str.to_owned()));
    }
    let start_x = parse_f64(moveto_parts[0])?;
    let start_y = parse_f64(moveto_parts[1])?;
    let start = Point { x: start_x, y: start_y };

    // Now at '(' - find the matching ')'
    let after_paren = &rest[moveto_end + 1..];
    let paren_close = after_paren.find(')').ok_or(EdgeParseError::UnexpectedEof)?;
    let inside_paren = &after_paren[..paren_close];

    // Inside parens format: [extra_coord_pair?] ';' cubic_data [quadratic_tail]
    // Split on ';' to separate potential extra pair from cubic data
    let parts: Vec<&str> = inside_paren.split(';').collect();
    if parts.len() < 2 {
        return Err(EdgeParseError::UnexpectedEof);
    }

    // parts[0] might be an extra coordinate pair (like "8360,5507") or empty
    // parts[1] is the cubic data (possibly followed by quadratic tail)
    let cubic_data_with_tail = parts[1];

    // Find where quadratic tail starts
    let quad_start = find_quadratic_tail_start(cubic_data_with_tail);
    let cubic_data = &cubic_data_with_tail[..quad_start];

    // Parse cubic data: space-separated coordinate pairs
    let coord_pairs = parse_coord_pairs(cubic_data)?;

    // Each cubic needs 3 pairs = 6 numbers (ctrl1, ctrl2, end)
    if coord_pairs.len() % 3 != 0 {
        return Err(EdgeParseError::MalformedCubic(coord_pairs.len()));
    }

    // Build cubic segments from the coordinate pairs
    let mut cubics = Vec::new();
    let mut current = start;
    for chunk in coord_pairs.chunks(3) {
        let ctrl1 = Point { x: chunk[0].0, y: chunk[0].1 };
        let ctrl2 = Point { x: chunk[1].0, y: chunk[1].1 };
        let end = Point { x: chunk[2].0, y: chunk[2].1 };
        cubics.push(CubicSegment { control1: ctrl1, control2: ctrl2, end });
        current = end;
    }

    Ok(ParsedEdge { start, cubics })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Real edge strings from a CS6-era FLA (DOMDocument.xml)
    const REAL_EDGES: &[&str] = &[
        r#"!8190 5402(;9125,5402 9317,5102 9735,4365q8190 5402Q8537 5402q8767 5344Q8992 5286q9156 5160Q9304 5046q9449 4838Q9548 4695q9735 4365);"#,
        r#"!9735 4365(;9947,3990 10242,3645 10270,3225q9735 4365Q9841 4178q10072 3812Q10252 3488q10270 3225);"#,
        r#"!10270 3225(;10285,3225 10270,3210 10320,3210q10270 3225Q10273 3225q10282 3218Q10291 3210q10320 3210);"#,
        r#"!10320 3210(;10370,3210 10370,3252 10370,3302q10320 3210Q10354 3210q10364 3237Q10370 3254q10370 3302);"#,
        r#"!10370 3302(;10370,3617 9902,4292 9785,4480q10370 3302Q10370 3501q10121 3939Q10116 3949q9785 4480);"#,
        r#"!9785 4480(;9662,4672 9525,4875 9380,5065q9785 4480Q9568 4818q9380 5065);"#,
        r#"!9380 5065(;9322,5140 9230,5265 8990,5362q9380 5065Q9310 5156q9253 5205Q9146 5299q8990 5362)8990,5362;"#,
        r#"!8990 5362(;8937,5382 8660,5507 8360,5507q8990 5362Q8817 5427q8768 5442Q8549 5507q8360 5507);"#,
        r#"!8360 5507(8360,5507;7985,5507 7792,5450 7655,5442q8360 5507Q8135 5507q7918 5478Q7658 5442q7655 5442);"#,
        r#"!7655 5442(7655,5442;7350,5427 7037,5400 6945,5377q7655 5442Q7419 5430q7220 5413Q7012 5394q6945 5377);"#,
        r#"!6945 5377(6945,5377;6860,5357 6247,5265 6190,5247q6945 5377Q6907 5368q6557 5311Q6221 5257q6190 5247);"#,
        r#"!6190 5247(;6123,5226 6003,5184 5841,5126q6190 5247Q6051 5204q5841 5126);"#,
        r#"!5841 5126(;5814,5114 5775,5100 5734,5085q5841 5126 5734 5085);"#,
        r#"!5734 5085(;5329,4930 4772,4701 4620,4500q5734 5085Q5438 4968q4872 4690Q4620 4500);"#,
        r#"!0 0(;100,0 100,100 0,100q0 0Q50 0q100 50Q100 100q50 100Q0 100q0 50Q0 0);"#,
        r#"!10 20(10,20;30,20 50,40 70,40q10 20Q20 20q40 30Q50 40q60 40Q70 40);"#,
    ];

    #[test]
    fn parse_all_real_edges() {
        for (i, edge_str) in REAL_EDGES.iter().enumerate() {
            let result = parse_cubics_edge(edge_str);
            assert!(result.is_ok(), "Edge {} failed to parse: {:?} -> {:?}", i, edge_str, result);
            let parsed = result.unwrap();
            assert!(!parsed.cubics.is_empty(), "Edge {} produced no cubics", i);
        }
    }

    #[test]
    fn first_edge_has_expected_cubic_count() {
        let parsed = parse_cubics_edge(REAL_EDGES[0]).unwrap();
        assert_eq!(parsed.cubics.len(), 1);
        assert_eq!(parsed.start.x, 8190.0);
        assert_eq!(parsed.start.y, 5402.0);
        assert_eq!(parsed.cubics[0].end.x, 9735.0);
        assert_eq!(parsed.cubics[0].end.y, 4365.0);
    }

    #[test]
    fn edge_with_extra_coord_pair_parses() {
        let parsed = parse_cubics_edge(REAL_EDGES[8]).unwrap();
        assert!(!parsed.cubics.is_empty());
        assert_eq!(parsed.start.x, 8360.0);
        assert_eq!(parsed.start.y, 5507.0);
    }

    #[test]
    fn quadratic_tail_is_discarded() {
        let parsed = parse_cubics_edge(REAL_EDGES[0]).unwrap();
        assert_eq!(parsed.cubics.len(), 1);
    }

    #[test]
    fn trailing_coords_after_closing_paren_ignored() {
        let parsed = parse_cubics_edge(REAL_EDGES[6]).unwrap();
        assert!(!parsed.cubics.is_empty());
    }

    #[test]
    fn single_cubic_with_extra_pair() {
        let parsed = parse_cubics_edge(r#"!10 20(10,20;30,20 50,40 70,40q10 20Q20 20q40 30Q50 40q60 40Q70 40);"#).unwrap();
        assert_eq!(parsed.cubics.len(), 1);
        assert_eq!(parsed.cubics[0].end.x, 70.0);
        assert_eq!(parsed.cubics[0].end.y, 40.0);
    }

    #[test]
    fn multiple_cubics_in_one_edge() {
        let s = r#"!0 0(;10,0 20,0 30,0 40,0 50,0 60,0q0 0Q5 0q10 5Q15 5q20 5Q25 5);"#;
        let parsed = parse_cubics_edge(s).unwrap();
        assert_eq!(parsed.cubics.len(), 2);
    }
}