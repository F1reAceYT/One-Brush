//! Geometry conversion for FLA import.
//!
//! Flash's native shape curves are quadratic Beziers (one control point) -- confirmed via
//! ActionScript's `Graphics.curveTo(controlX, controlY, anchorX, anchorY)`, which is the API
//! Animate's own edge-editing tools (`Edge.getControl(0..2)`) are built on. This module elevates
//! a quadratic segment to the cubic form the rest of the pipeline (BezierPath component, Lyon
//! tessellation) is expected to standardize on, so only one curve representation exists past the
//! import boundary.
//!
//! This math is independent of how the quadratic control points were obtained -- it's correct
//! regardless of whatever the raw `edges=""` string tokenizer turns out to look like once
//! verified against a real file (see `fla/edge.rs`, currently a stub pending that).

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// A single cubic Bezier segment: from the previous segment's end point, through two control
/// points, to `end`. This is the canonical form `BezierPath` stores -- quadratic segments from
/// FLA import get elevated to this on the way in, so nothing downstream needs to special-case
/// curve degree.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CubicSegment {
    pub control1: Point,
    pub control2: Point,
    pub end: Point,
}

/// Exact degree elevation from quadratic to cubic (not an approximation -- a quadratic curve is
/// already a cubic curve whose control points happen to coincide at this specific position, so
/// this loses no precision). Standard formula: for quadratic control point `q` between `start`
/// and `end`, the equivalent cubic control points are `start + 2/3*(q - start)` and
/// `end + 2/3*(q - end)`.
pub fn quadratic_to_cubic(start: Point, control: Point, end: Point) -> CubicSegment {
    let control1 = Point { x: start.x + (2.0 / 3.0) * (control.x - start.x), y: start.y + (2.0 / 3.0) * (control.y - start.y) };
    let control2 = Point { x: end.x + (2.0 / 3.0) * (control.x - end.x), y: end.y + (2.0 / 3.0) * (control.y - end.y) };
    CubicSegment { control1, control2, end }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elevation_preserves_endpoints() {
        let start = Point { x: 0.0, y: 0.0 };
        let control = Point { x: 50.0, y: 100.0 };
        let end = Point { x: 100.0, y: 0.0 };
        let cubic = quadratic_to_cubic(start, control, end);
        // The elevated cubic must reproduce the same curve, which we can check by sampling
        // both parameterizations at t=0.5 and comparing (they should match to float precision
        // since elevation is exact, not approximate).
        let quad_mid = Point {
            x: (1.0 - 0.5f64).powi(2) * start.x + 2.0 * 0.5 * (1.0 - 0.5) * control.x + 0.5f64.powi(2) * end.x,
            y: (1.0 - 0.5f64).powi(2) * start.y + 2.0 * 0.5 * (1.0 - 0.5) * control.y + 0.5f64.powi(2) * end.y,
        };
        let t = 0.5f64;
        let mt = 1.0 - t;
        let cubic_mid = Point {
            x: mt.powi(3) * start.x + 3.0 * mt.powi(2) * t * cubic.control1.x + 3.0 * mt * t.powi(2) * cubic.control2.x + t.powi(3) * cubic.end.x,
            y: mt.powi(3) * start.y + 3.0 * mt.powi(2) * t * cubic.control1.y + 3.0 * mt * t.powi(2) * cubic.control2.y + t.powi(3) * cubic.end.y,
        };
        assert!((quad_mid.x - cubic_mid.x).abs() < 1e-9);
        assert!((quad_mid.y - cubic_mid.y).abs() < 1e-9);
    }
}
