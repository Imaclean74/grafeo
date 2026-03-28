//! SQL/PGQ edge case tests for pattern matching and expression evaluation.
//!
//! Covers anonymous nodes, variable-length path variants, numeric edge cases,
//! string edge cases, BETWEEN/LIKE/IN edge cases, list literals, NULL
//! propagation, duplicate nodes, and multi-pattern cross products.
//!
//! Run with:
//! ```bash
//! cargo test -p grafeo-engine --features sql-pgq --test sql_pgq_edge_cases
//! ```

#![cfg(feature = "sql-pgq")]

use std::sync::Arc;

use grafeo_common::types::Value;
use grafeo_engine::GrafeoDB;

// ============================================================================
// Test Fixtures
// ============================================================================

/// Creates a small social network for general edge-case testing.
///
/// Nodes:
/// - Alix (Person, age: 30, city: "Amsterdam", val: 10)
/// - Gus (Person, age: 25, city: "Berlin", val: 0)
/// - Vincent (Person, age: 28, city: "Paris")  [no `val` property]
///
/// Edges:
/// - Alix -KNOWS-> Gus (since: 2020)
/// - Gus -KNOWS-> Vincent (since: 2022)
fn create_small_network() -> GrafeoDB {
    let db = GrafeoDB::new_in_memory();
    let session = db.session();

    let alix = session.create_node_with_props(
        &["Person"],
        [
            ("name", Value::String("Alix".into())),
            ("age", Value::Int64(30)),
            ("city", Value::String("Amsterdam".into())),
            ("val", Value::Int64(10)),
        ],
    );
    let gus = session.create_node_with_props(
        &["Person"],
        [
            ("name", Value::String("Gus".into())),
            ("age", Value::Int64(25)),
            ("city", Value::String("Berlin".into())),
            ("val", Value::Int64(0)),
        ],
    );
    let vincent = session.create_node_with_props(
        &["Person"],
        [
            ("name", Value::String("Vincent".into())),
            ("age", Value::Int64(28)),
            ("city", Value::String("Paris".into())),
        ],
    );

    let e1 = session.create_edge(alix, gus, "KNOWS");
    db.set_edge_property(e1, "since", Value::Int64(2020));
    let e2 = session.create_edge(gus, vincent, "KNOWS");
    db.set_edge_property(e2, "since", Value::Int64(2022));

    db
}

/// Creates a chain graph for variable-length path tests.
///
/// A -LINK-> B -LINK-> C -LINK-> D -LINK-> E
fn create_chain() -> GrafeoDB {
    let db = GrafeoDB::new_in_memory();
    let session = db.session();

    let na = session.create_node_with_props(&["Node"], [("name", Value::String("A".into()))]);
    let nb = session.create_node_with_props(&["Node"], [("name", Value::String("B".into()))]);
    let nc = session.create_node_with_props(&["Node"], [("name", Value::String("C".into()))]);
    let nd = session.create_node_with_props(&["Node"], [("name", Value::String("D".into()))]);
    let ne = session.create_node_with_props(&["Node"], [("name", Value::String("E".into()))]);

    session.create_edge(na, nb, "LINK");
    session.create_edge(nb, nc, "LINK");
    session.create_edge(nc, nd, "LINK");
    session.create_edge(nd, ne, "LINK");

    db
}

/// Creates a graph with duplicate nodes (same label and properties).
///
/// - Jules (Person, age: 40, city: "Prague")
/// - Jules (Person, age: 40, city: "Prague") [second, identical properties]
fn create_duplicate_nodes() -> GrafeoDB {
    let db = GrafeoDB::new_in_memory();
    let session = db.session();

    session.create_node_with_props(
        &["Person"],
        [
            ("name", Value::String("Jules".into())),
            ("age", Value::Int64(40)),
            ("city", Value::String("Prague".into())),
        ],
    );
    session.create_node_with_props(
        &["Person"],
        [
            ("name", Value::String("Jules".into())),
            ("age", Value::Int64(40)),
            ("city", Value::String("Prague".into())),
        ],
    );

    db
}

/// Creates a graph with nodes carrying edge cases in their string properties.
///
/// - Alix (Person, name: "Alix", bio: "")             [empty string]
/// - Gus (Person, name: "Gus", bio: "short")
/// - Mia (Person, name: "Mia", bio: <1000+ char string>)
fn create_string_edge_cases() -> GrafeoDB {
    let db = GrafeoDB::new_in_memory();
    let session = db.session();

    session.create_node_with_props(
        &["Person"],
        [
            ("name", Value::String("Alix".into())),
            ("bio", Value::String(String::new().into())),
        ],
    );
    session.create_node_with_props(
        &["Person"],
        [
            ("name", Value::String("Gus".into())),
            ("bio", Value::String("short".into())),
        ],
    );

    let long_string = "x".repeat(1200);
    session.create_node_with_props(
        &["Person"],
        [
            ("name", Value::String("Mia".into())),
            ("bio", Value::String(long_string.into())),
        ],
    );

    db
}

/// Creates a graph for cross-product (multi-pattern) testing.
///
/// - Alix (Person, name: "Alix")
/// - Gus (Person, name: "Gus")
/// - Amsterdam (City, name: "Amsterdam")
/// - Berlin (City, name: "Berlin")
/// - Prague (City, name: "Prague")
fn create_multi_label_for_cross() -> GrafeoDB {
    let db = GrafeoDB::new_in_memory();
    let session = db.session();

    session.create_node_with_props(&["Person"], [("name", Value::String("Alix".into()))]);
    session.create_node_with_props(&["Person"], [("name", Value::String("Gus".into()))]);
    session.create_node_with_props(&["City"], [("name", Value::String("Amsterdam".into()))]);
    session.create_node_with_props(&["City"], [("name", Value::String("Berlin".into()))]);
    session.create_node_with_props(&["City"], [("name", Value::String("Prague".into()))]);

    db
}

// ============================================================================
// Anonymous nodes (no variable)
// ============================================================================

#[test]
fn test_anonymous_node_count() {
    let db = create_small_network();
    let session = db.session();

    // Anonymous node: no variable, just a label filter.
    // We cannot reference properties from the anonymous node in COLUMNS,
    // but we can still count matching rows.
    let result = session
        .execute_sql(
            "SELECT COUNT(*) AS cnt FROM GRAPH_TABLE (
                MATCH (:Person)
                COLUMNS (1 AS dummy)
            )",
        )
        .unwrap();

    assert_eq!(result.row_count(), 1);
    assert_eq!(result.rows[0][0], Value::Int64(3), "3 Person nodes exist");
}

#[test]
fn test_anonymous_nodes_in_edge_pattern() {
    let db = create_small_network();
    let session = db.session();

    // Both endpoints are anonymous, but the edge has a variable.
    let result = session
        .execute_sql(
            "SELECT COUNT(*) AS cnt FROM GRAPH_TABLE (
                MATCH (:Person)-[e:KNOWS]->(:Person)
                COLUMNS (e.since AS since)
            )",
        )
        .unwrap();

    assert_eq!(result.row_count(), 1);
    // 2 KNOWS edges
    assert_eq!(result.rows[0][0], Value::Int64(2));
}

// ============================================================================
// Variable-length path hop range variants
// ============================================================================

#[test]
fn test_exact_three_hops() {
    let db = create_chain();
    let session = db.session();

    // *3 means exactly 3 hops: A->B->C->D
    let result = session
        .execute_sql(
            "SELECT * FROM GRAPH_TABLE (
                MATCH (a:Node {name: 'A'})-[p:LINK*3..3]->(b:Node)
                COLUMNS (a.name AS source, b.name AS target)
            )",
        )
        .unwrap();

    assert_eq!(result.row_count(), 1);
    assert_eq!(result.rows[0][1], Value::String("D".into()));
}

#[test]
fn test_exact_three_hops_shorthand() {
    let db = create_chain();
    let session = db.session();

    // *3 (without ..) should mean exactly 3 hops (min=3, max=3)
    let result = session
        .execute_sql(
            "SELECT * FROM GRAPH_TABLE (
                MATCH (a:Node {name: 'A'})-[p:LINK*3]->(b:Node)
                COLUMNS (a.name AS source, b.name AS target)
            )",
        )
        .unwrap();

    assert_eq!(result.row_count(), 1);
    assert_eq!(result.rows[0][1], Value::String("D".into()));
}

#[test]
#[ignore = "zero-hop variable-length paths not yet supported in SQL/PGQ executor"]
fn test_zero_to_two_hops_includes_self() {
    let db = create_chain();
    let session = db.session();

    // *0..2: 0 hops (A itself), 1 hop (B), 2 hops (C)
    let result = session
        .execute_sql(
            "SELECT * FROM GRAPH_TABLE (
                MATCH (a:Node {name: 'A'})-[p:LINK*0..2]->(b:Node)
                COLUMNS (a.name AS source, b.name AS target)
            )",
        )
        .unwrap();

    // 0 hops: A, 1 hop: B, 2 hops: C = 3 rows
    assert_eq!(result.row_count(), 3);

    let targets: Vec<&str> = result.rows.iter().filter_map(|r| r[1].as_str()).collect();
    assert!(
        targets.contains(&"A"),
        "0-hop should return the start node itself"
    );
    assert!(targets.contains(&"B"), "1-hop should reach B");
    assert!(targets.contains(&"C"), "2-hop should reach C");
}

#[test]
#[ignore = "SQL/PGQ translator defaults unbounded max_hops to Some(1), needs None passthrough"]
fn test_unbounded_upper_limit() {
    let db = create_chain();
    let session = db.session();

    // *1.. means one or more hops, no upper bound.
    // From A: B (1 hop), C (2), D (3), E (4)
    let result = session
        .execute_sql(
            "SELECT * FROM GRAPH_TABLE (
                MATCH (a:Node {name: 'A'})-[p:LINK*1..]->(b:Node)
                COLUMNS (a.name AS source, b.name AS target)
            )",
        )
        .unwrap();

    assert_eq!(result.row_count(), 4);

    let targets: Vec<&str> = result.rows.iter().filter_map(|r| r[1].as_str()).collect();
    assert!(targets.contains(&"B"));
    assert!(targets.contains(&"C"));
    assert!(targets.contains(&"D"));
    assert!(targets.contains(&"E"));
}

// ============================================================================
// Undirected variable-length edges
// ============================================================================

#[test]
fn test_undirected_variable_length() {
    let db = create_chain();
    let session = db.session();

    // Undirected with hops: from C, 1..2 hops in any direction (Walk mode: revisits allowed)
    // 1 hop: B (C~B), D (C~D)
    // 2 hops: A (C~B~A), C (C~B~C, back to self), C (C~D~C, back to self), E (C~D~E)
    let result = session
        .execute_sql(
            "SELECT * FROM GRAPH_TABLE (
                MATCH (a:Node {name: 'C'})-[p:LINK*1..2]-(b:Node)
                COLUMNS (a.name AS source, b.name AS target)
            )",
        )
        .unwrap();

    // Walk mode allows revisiting: B, D (1-hop) + A, C, C, E (2-hop) = 6
    assert_eq!(result.row_count(), 6);

    let targets: Vec<&str> = result.rows.iter().filter_map(|r| r[1].as_str()).collect();
    assert!(targets.contains(&"B"), "1 hop back should reach B");
    assert!(targets.contains(&"D"), "1 hop forward should reach D");
    assert!(targets.contains(&"A"), "2 hops back should reach A");
    assert!(targets.contains(&"E"), "2 hops forward should reach E");
    assert!(
        targets.contains(&"C"),
        "2 hops should revisit C via both directions"
    );
}

// ============================================================================
// Numeric edge cases
// ============================================================================

#[test]
fn test_division_by_zero_returns_null() {
    let db = create_small_network();
    let session = db.session();

    // Division by zero in COLUMNS: n.val / 0 should produce Null (checked_div returns None)
    let result = session
        .execute_sql(
            "SELECT * FROM GRAPH_TABLE (
                MATCH (n:Person)
                WHERE n.name = 'Alix'
                COLUMNS (n.name AS name, n.val / 0 AS divided)
            )",
        )
        .unwrap();

    assert_eq!(result.row_count(), 1);
    assert!(
        result.rows[0][1].is_null(),
        "Division by zero should produce Null, got: {:?}",
        result.rows[0][1]
    );
}

#[test]
fn test_negative_arithmetic() {
    let db = create_small_network();
    let session = db.session();

    // -5 * 3 = -15 as a computed column
    let result = session
        .execute_sql(
            "SELECT * FROM GRAPH_TABLE (
                MATCH (n:Person)
                WHERE n.name = 'Alix'
                COLUMNS (n.name AS name, -5 * 3 AS neg_product)
            )",
        )
        .unwrap();

    assert_eq!(result.row_count(), 1);
    assert_eq!(result.rows[0][1], Value::Int64(-15));
}

#[test]
fn test_large_integer_arithmetic() {
    let db = create_small_network();
    let session = db.session();

    // Large integer: 1_000_000_000 * 1000 = 1_000_000_000_000
    let result = session
        .execute_sql(
            "SELECT * FROM GRAPH_TABLE (
                MATCH (n:Person)
                WHERE n.name = 'Alix'
                COLUMNS (n.name AS name, 1000000000 * 1000 AS big)
            )",
        )
        .unwrap();

    assert_eq!(result.row_count(), 1);
    assert_eq!(result.rows[0][1], Value::Int64(1_000_000_000_000));
}

#[test]
fn test_float_precision() {
    let db = create_small_network();
    let session = db.session();

    // 0.1 + 0.2 should be close to 0.3 (IEEE 754 precision)
    let result = session
        .execute_sql(
            "SELECT * FROM GRAPH_TABLE (
                MATCH (n:Person)
                WHERE n.name = 'Alix'
                COLUMNS (n.name AS name, 0.1 + 0.2 AS sum)
            )",
        )
        .unwrap();

    assert_eq!(result.row_count(), 1);
    match &result.rows[0][1] {
        Value::Float64(f) => {
            assert!(
                (*f - 0.3).abs() < 1e-10,
                "0.1 + 0.2 should be approximately 0.3, got {f}"
            );
            // Verify it is NOT exactly 0.3 (classic IEEE 754 quirk)
            assert_ne!(*f, 0.3_f64, "IEEE 754: 0.1+0.2 should not be exactly 0.3");
        }
        other => panic!("Expected Float64, got: {other:?}"),
    }
}

#[test]
fn test_negative_value_in_where() {
    let db = create_small_network();
    let session = db.session();

    // Filter with a negative literal: age > -1 should match all
    let result = session
        .execute_sql(
            "SELECT * FROM GRAPH_TABLE (
                MATCH (n:Person)
                WHERE n.age > -1
                COLUMNS (n.name AS name)
            )",
        )
        .unwrap();

    assert_eq!(result.row_count(), 3, "All persons have age > -1");
}

// ============================================================================
// String edge cases
// ============================================================================

#[test]
fn test_empty_string_property() {
    let db = create_string_edge_cases();
    let session = db.session();

    // Alix has bio = "" (empty string)
    let result = session
        .execute_sql(
            "SELECT * FROM GRAPH_TABLE (
                MATCH (n:Person)
                WHERE n.bio = ''
                COLUMNS (n.name AS name, n.bio AS bio)
            )",
        )
        .unwrap();

    assert_eq!(result.row_count(), 1);
    assert_eq!(result.rows[0][0], Value::String("Alix".into()));
    assert_eq!(result.rows[0][1], Value::String(String::new().into()));
}

#[test]
fn test_escaped_single_quote_in_string() {
    let db = GrafeoDB::new_in_memory();
    let session = db.session();

    session.create_node_with_props(&["Person"], [("name", Value::String("O'Brien".into()))]);

    let result = session
        .execute_sql(
            "SELECT * FROM GRAPH_TABLE (
                MATCH (n:Person)
                WHERE n.name = 'O\\'Brien'
                COLUMNS (n.name AS name)
            )",
        )
        .unwrap();

    assert_eq!(result.row_count(), 1);
    assert_eq!(result.rows[0][0], Value::String("O'Brien".into()));
}

#[test]
fn test_very_long_string_property() {
    let db = create_string_edge_cases();
    let session = db.session();

    // Mia has a 1200-char bio
    let result = session
        .execute_sql(
            "SELECT * FROM GRAPH_TABLE (
                MATCH (n:Person)
                WHERE n.name = 'Mia'
                COLUMNS (n.name AS name, n.bio AS bio)
            )",
        )
        .unwrap();

    assert_eq!(result.row_count(), 1);
    match &result.rows[0][1] {
        Value::String(s) => assert_eq!(s.len(), 1200, "Bio should be 1200 chars"),
        other => panic!("Expected String, got: {other:?}"),
    }
}

// ============================================================================
// BETWEEN edge cases
// ============================================================================

#[test]
fn test_between_reversed_bounds() {
    let db = create_small_network();
    let session = db.session();

    // BETWEEN 35 AND 25: per SQL standard, when low > high, no rows match.
    // This desugars to (age >= 35 AND age <= 25), which is impossible.
    let result = session
        .execute_sql(
            "SELECT * FROM GRAPH_TABLE (
                MATCH (n:Person)
                WHERE n.age BETWEEN 35 AND 25
                COLUMNS (n.name AS name)
            )",
        )
        .unwrap();

    assert_eq!(
        result.row_count(),
        0,
        "Reversed BETWEEN bounds should return 0 rows"
    );
}

#[test]
fn test_between_equal_bounds() {
    let db = create_small_network();
    let session = db.session();

    // BETWEEN 30 AND 30: should match exactly age=30 (Alix)
    let result = session
        .execute_sql(
            "SELECT * FROM GRAPH_TABLE (
                MATCH (n:Person)
                WHERE n.age BETWEEN 30 AND 30
                COLUMNS (n.name AS name)
            )",
        )
        .unwrap();

    assert_eq!(result.row_count(), 1);
    assert_eq!(result.rows[0][0], Value::String("Alix".into()));
}

#[test]
fn test_between_negative_bounds() {
    let db = create_small_network();
    let session = db.session();

    // BETWEEN -10 AND 10: matches val property (Alix=10, Gus=0, Vincent has no val)
    let result = session
        .execute_sql(
            "SELECT * FROM GRAPH_TABLE (
                MATCH (n:Person)
                WHERE n.val BETWEEN -10 AND 10
                COLUMNS (n.name AS name, n.val AS val)
            )",
        )
        .unwrap();

    // Alix (val=10) and Gus (val=0) are in range. Vincent has no val (Null).
    assert_eq!(result.row_count(), 2);
}

// ============================================================================
// LIKE edge cases
// ============================================================================

#[test]
fn test_like_single_char_wildcard() {
    let db = create_small_network();
    let session = db.session();

    // LIKE '_lix': underscore matches exactly one character
    let result = session
        .execute_sql(
            "SELECT * FROM GRAPH_TABLE (
                MATCH (n:Person)
                WHERE n.name LIKE '_lix'
                COLUMNS (n.name AS name)
            )",
        )
        .unwrap();

    assert_eq!(result.row_count(), 1);
    assert_eq!(result.rows[0][0], Value::String("Alix".into()));
}

#[test]
fn test_like_percent_in_middle() {
    let db = create_small_network();
    let session = db.session();

    // LIKE 'A%x': starts with A, ends with x
    let result = session
        .execute_sql(
            "SELECT * FROM GRAPH_TABLE (
                MATCH (n:Person)
                WHERE n.name LIKE 'A%x'
                COLUMNS (n.name AS name)
            )",
        )
        .unwrap();

    assert_eq!(result.row_count(), 1);
    assert_eq!(result.rows[0][0], Value::String("Alix".into()));
}

#[test]
fn test_like_no_wildcards_exact_match() {
    let db = create_small_network();
    let session = db.session();

    // LIKE 'Alix' (no wildcards): should be an exact match
    let result = session
        .execute_sql(
            "SELECT * FROM GRAPH_TABLE (
                MATCH (n:Person)
                WHERE n.name LIKE 'Alix'
                COLUMNS (n.name AS name)
            )",
        )
        .unwrap();

    assert_eq!(result.row_count(), 1);
    assert_eq!(result.rows[0][0], Value::String("Alix".into()));
}

#[test]
fn test_like_empty_pattern() {
    let db = create_string_edge_cases();
    let session = db.session();

    // LIKE '': only matches empty string properties
    let result = session
        .execute_sql(
            "SELECT * FROM GRAPH_TABLE (
                MATCH (n:Person)
                WHERE n.bio LIKE ''
                COLUMNS (n.name AS name)
            )",
        )
        .unwrap();

    // Only Alix has bio = ""
    assert_eq!(result.row_count(), 1);
    assert_eq!(result.rows[0][0], Value::String("Alix".into()));
}

// ============================================================================
// List edge cases
// ============================================================================

#[test]
fn test_empty_list_literal_in_columns() {
    let db = create_small_network();
    let session = db.session();

    let result = session
        .execute_sql(
            "SELECT * FROM GRAPH_TABLE (
                MATCH (n:Person)
                WHERE n.name = 'Alix'
                COLUMNS (n.name AS name, [] AS empty_list)
            )",
        )
        .unwrap();

    assert_eq!(result.row_count(), 1);
    assert_eq!(
        result.rows[0][1],
        Value::List(Arc::from(Vec::<Value>::new().as_slice())),
        "Empty list literal should produce an empty list"
    );
}

#[test]
fn test_nested_list_in_columns() {
    let db = create_small_network();
    let session = db.session();

    let result = session
        .execute_sql(
            "SELECT * FROM GRAPH_TABLE (
                MATCH (n:Person)
                WHERE n.name = 'Alix'
                COLUMNS (n.name AS name, [[1, 2], [3, 4]] AS nested)
            )",
        )
        .unwrap();

    assert_eq!(result.row_count(), 1);
    let expected = Value::List(Arc::from(
        vec![
            Value::List(Arc::from(vec![Value::Int64(1), Value::Int64(2)].as_slice())),
            Value::List(Arc::from(vec![Value::Int64(3), Value::Int64(4)].as_slice())),
        ]
        .as_slice(),
    ));
    assert_eq!(result.rows[0][1], expected);
}

#[test]
fn test_mixed_type_list_in_columns() {
    let db = create_small_network();
    let session = db.session();

    let result = session
        .execute_sql(
            "SELECT * FROM GRAPH_TABLE (
                MATCH (n:Person)
                WHERE n.name = 'Alix'
                COLUMNS (n.name AS name, [1, 'two', TRUE] AS mixed)
            )",
        )
        .unwrap();

    assert_eq!(result.row_count(), 1);
    let expected = Value::List(Arc::from(
        vec![
            Value::Int64(1),
            Value::String("two".into()),
            Value::Bool(true),
        ]
        .as_slice(),
    ));
    assert_eq!(result.rows[0][1], expected);
}

// ============================================================================
// IN with empty list
// ============================================================================

#[test]
fn test_in_with_empty_list() {
    let db = create_small_network();
    let session = db.session();

    // WHERE n.name IN [] should return 0 rows
    let result = session
        .execute_sql(
            "SELECT * FROM GRAPH_TABLE (
                MATCH (n:Person)
                WHERE n.name IN []
                COLUMNS (n.name AS name)
            )",
        )
        .unwrap();

    assert_eq!(
        result.row_count(),
        0,
        "IN with empty list should match nothing"
    );
}

// ============================================================================
// Expression result as NULL (missing property propagation)
// ============================================================================

#[test]
fn test_addition_with_missing_property_is_null() {
    let db = create_small_network();
    let session = db.session();

    // Vincent has no `val` property, so n.val + 5 should be Null for Vincent.
    // Alix has val=10 so val+5=15, Gus has val=0 so val+5=5.
    let result = session
        .execute_sql(
            "SELECT * FROM GRAPH_TABLE (
                MATCH (n:Person)
                COLUMNS (n.name AS name, n.val + 5 AS computed)
            )",
        )
        .unwrap();

    assert_eq!(result.row_count(), 3);

    let vincent_row = result
        .rows
        .iter()
        .find(|r| r[0].as_str() == Some("Vincent"))
        .expect("Vincent should appear");
    assert!(
        vincent_row[1].is_null(),
        "Missing property in arithmetic should produce Null, got: {:?}",
        vincent_row[1]
    );

    let alix_row = result
        .rows
        .iter()
        .find(|r| r[0].as_str() == Some("Alix"))
        .expect("Alix should appear");
    assert_eq!(alix_row[1], Value::Int64(15));
}

#[test]
fn test_is_null_on_computed_expression() {
    let db = create_small_network();
    let session = db.session();

    // Filter rows where the computed expression (val + 5) IS NULL
    let result = session
        .execute_sql(
            "SELECT * FROM GRAPH_TABLE (
                MATCH (n:Person)
                WHERE (n.val + 5) IS NULL
                COLUMNS (n.name AS name)
            )",
        )
        .unwrap();

    // Only Vincent lacks the val property
    assert_eq!(result.row_count(), 1);
    assert_eq!(result.rows[0][0], Value::String("Vincent".into()));
}

// ============================================================================
// Duplicate nodes with identical properties
// ============================================================================

#[test]
fn test_duplicate_nodes_both_returned() {
    let db = create_duplicate_nodes();
    let session = db.session();

    let result = session
        .execute_sql(
            "SELECT * FROM GRAPH_TABLE (
                MATCH (n:Person {name: 'Jules'})
                COLUMNS (n.name AS name, n.age AS age, n.city AS city)
            )",
        )
        .unwrap();

    assert_eq!(
        result.row_count(),
        2,
        "Both duplicate nodes should be returned"
    );
    for row in &result.rows {
        assert_eq!(row[0], Value::String("Jules".into()));
        assert_eq!(row[1], Value::Int64(40));
        assert_eq!(row[2], Value::String("Prague".into()));
    }
}

// ============================================================================
// Pattern with multiple comma-separated patterns (cross product)
// ============================================================================

#[test]
fn test_two_comma_separated_patterns_cross_product() {
    let db = create_multi_label_for_cross();
    let session = db.session();

    // 2 Person x 3 City = 6 rows
    let result = session
        .execute_sql(
            "SELECT * FROM GRAPH_TABLE (
                MATCH (p:Person), (c:City)
                COLUMNS (p.name AS person, c.name AS city)
            )",
        )
        .unwrap();

    assert_eq!(
        result.row_count(),
        6,
        "2 persons x 3 cities = 6 cross product rows"
    );
}

#[test]
fn test_three_comma_separated_patterns_cross_product() {
    let db = GrafeoDB::new_in_memory();
    let session = db.session();

    // Create 2 Person, 2 City, 2 Tag nodes
    session.create_node_with_props(&["Person"], [("name", Value::String("Alix".into()))]);
    session.create_node_with_props(&["Person"], [("name", Value::String("Gus".into()))]);
    session.create_node_with_props(&["City"], [("name", Value::String("Amsterdam".into()))]);
    session.create_node_with_props(&["City"], [("name", Value::String("Berlin".into()))]);
    session.create_node_with_props(&["Tag"], [("name", Value::String("rust".into()))]);
    session.create_node_with_props(&["Tag"], [("name", Value::String("graph".into()))]);

    // 2 x 2 x 2 = 8 rows
    let result = session
        .execute_sql(
            "SELECT * FROM GRAPH_TABLE (
                MATCH (a:Person), (b:City), (c:Tag)
                COLUMNS (a.name AS person, b.name AS city, c.name AS tag)
            )",
        )
        .unwrap();

    assert_eq!(
        result.row_count(),
        8,
        "2 persons x 2 cities x 2 tags = 8 cross product rows"
    );
}

// ============================================================================
// Edge with property filter but no variable or type
// ============================================================================

#[test]
#[ignore = "edge property map without variable or type not yet supported in SQL/PGQ executor"]
fn test_edge_with_property_no_variable_or_type() {
    let db = create_small_network();
    let session = db.session();

    // Inline edge property filter with no variable name and no type
    let result = session
        .execute_sql(
            "SELECT * FROM GRAPH_TABLE (
                MATCH (a:Person)-[{since: 2020}]->(b:Person)
                COLUMNS (a.name AS source, b.name AS target)
            )",
        )
        .unwrap();

    assert_eq!(result.row_count(), 1);
    assert_eq!(result.rows[0][0], Value::String("Alix".into()));
    assert_eq!(result.rows[0][1], Value::String("Gus".into()));
}
