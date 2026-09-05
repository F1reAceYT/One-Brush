//! Integration tests for the FLA edge parser.

use quad_engine_core::edge::{parse_cubics_edge, ParsedEdge};

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