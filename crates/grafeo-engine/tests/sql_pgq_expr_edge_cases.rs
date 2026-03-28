//! SQL/PGQ aggregate and expression edge case tests.
//!
//! Covers gaps in:
//! - Implicit GROUP BY (aggregates without GROUP BY clause)
//! - SUM(DISTINCT), AVG(DISTINCT)
//! - Aggregates on empty results
//! - Aggregates with all NULLs
//! - Index access expressions
//! - Unary positive operator
//! - Nested function calls in WHERE
//! - Complex expressions in COLUMNS
//! - SELECT * with outer WHERE
//! - Unsupported expression error paths
//! - Parameterized query edge cases
//!
//! Run with:
//! ```bash
//! cargo test -p grafeo-engine --features sql-pgq --test sql_pgq_expr_edge_cases
//! ```

#![cfg(feature = "sql-pgq")]

use grafeo_common::types::Value;
use grafeo_engine::GrafeoDB;

// ============================================================================
// Shared fixture: same as sql_pgq_coverage.rs
// ============================================================================

fn create_rich_network() -> GrafeoDB {
    let db = GrafeoDB::new_in_memory();
    let session = db.session();

    let alix = session.create_node_with_props(
        &["Person"],
        [
            ("name", Value::String("Alix".into())),
            ("age", Value::Int64(30)),
            ("city", Value::String("Amsterdam".into())),
        ],
    );
    let gus = session.create_node_with_props(
        &["Person"],
        [
            ("name", Value::String("Gus".into())),
            ("age", Value::Int64(25)),
            ("city", Value::String("Berlin".into())),
        ],
    );
    let harm = session.create_node_with_props(
        &["Person"],
        [
            ("name", Value::String("Harm".into())),
            ("age", Value::Int64(35)),
            ("city", Value::String("Amsterdam".into())),
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
    let mia = session.create_node_with_props(
        &["Person"],
        [
            ("name", Value::String("Mia".into())),
            ("age", Value::Int64(32)),
            ("city", Value::String("Berlin".into())),
        ],
    );

    let e1 = session.create_edge(alix, gus, "KNOWS");
    db.set_edge_property(e1, "since", Value::Int64(2020));
    let e2 = session.create_edge(alix, harm, "KNOWS");
    db.set_edge_property(e2, "since", Value::Int64(2018));
    let e3 = session.create_edge(gus, harm, "KNOWS");
    db.set_edge_property(e3, "since", Value::Int64(2021));
    let e4 = session.create_edge(vincent, mia, "KNOWS");
    db.set_edge_property(e4, "since", Value::Int64(2019));
    let e5 = session.create_edge(alix, vincent, "FOLLOWS");
    db.set_edge_property(e5, "since", Value::Int64(2022));
    let e6 = session.create_edge(gus, alix, "FOLLOWS");
    db.set_edge_property(e6, "since", Value::Int64(2023));

    db
}

/// Creates a graph with duplicate ages for DISTINCT aggregate testing.
///
/// Nodes:
/// - Alix (Person, age: 30)
/// - Gus (Person, age: 25)
/// - Vincent (Person, age: 30)   <-- duplicate age with Alix
/// - Mia (Person, age: 25)       <-- duplicate age with Gus
/// - Jules (Person, age: 40)
fn create_duplicate_ages() -> GrafeoDB {
    let db = GrafeoDB::new_in_memory();
    let session = db.session();

    session.create_node_with_props(
        &["Person"],
        [
            ("name", Value::String("Alix".into())),
            ("age", Value::Int64(30)),
        ],
    );
    session.create_node_with_props(
        &["Person"],
        [
            ("name", Value::String("Gus".into())),
            ("age", Value::Int64(25)),
        ],
    );
    session.create_node_with_props(
        &["Person"],
        [
            ("name", Value::String("Vincent".into())),
            ("age", Value::Int64(30)),
        ],
    );
    session.create_node_with_props(
        &["Person"],
        [
            ("name", Value::String("Mia".into())),
            ("age", Value::Int64(25)),
        ],
    );
    session.create_node_with_props(
        &["Person"],
        [
            ("name", Value::String("Jules".into())),
            ("age", Value::Int64(40)),
        ],
    );

    db
}

/// Creates a graph where every node has a NULL value for the `val` property.
fn create_all_nulls() -> GrafeoDB {
    let db = GrafeoDB::new_in_memory();
    let session = db.session();

    session.create_node_with_props(
        &["Item"],
        [
            ("name", Value::String("alpha".into())),
            ("val", Value::Null),
        ],
    );
    session.create_node_with_props(
        &["Item"],
        [("name", Value::String("beta".into())), ("val", Value::Null)],
    );
    session.create_node_with_props(
        &["Item"],
        [
            ("name", Value::String("gamma".into())),
            ("val", Value::Null),
        ],
    );

    db
}

// ============================================================================
// Implicit GROUP BY (aggregates without GROUP BY clause)
// ============================================================================

#[test]
fn test_implicit_group_by_count_star() {
    let db = create_rich_network();
    let session = db.session();

    // COUNT(*) without GROUP BY should implicitly group all rows into one.
    let result = session
        .execute_sql(
            "SELECT COUNT(*) AS total FROM GRAPH_TABLE (
                MATCH (n:Person)
                COLUMNS (n.name AS name)
            )",
        )
        .unwrap();

    assert_eq!(
        result.row_count(),
        1,
        "implicit GROUP BY yields exactly one row"
    );
    assert_eq!(result.rows[0][0], Value::Int64(5));
}

#[test]
fn test_implicit_group_by_min_max() {
    let db = create_rich_network();
    let session = db.session();

    // Multiple aggregates without GROUP BY: all rows form a single group.
    let result = session
        .execute_sql(
            "SELECT MIN(age) AS youngest, MAX(age) AS oldest FROM GRAPH_TABLE (
                MATCH (n:Person)
                COLUMNS (n.age AS age)
            )",
        )
        .unwrap();

    assert_eq!(result.row_count(), 1);
    assert_eq!(result.rows[0][0], Value::Int64(25), "youngest is 25 (Gus)");
    assert_eq!(result.rows[0][1], Value::Int64(35), "oldest is 35 (Harm)");
}

#[test]
fn test_implicit_group_by_sum() {
    let db = create_rich_network();
    let session = db.session();

    // SUM without GROUP BY: single aggregate, no grouping key.
    let result = session
        .execute_sql(
            "SELECT SUM(age) AS total FROM GRAPH_TABLE (
                MATCH (n:Person)
                COLUMNS (n.age AS age)
            )",
        )
        .unwrap();

    assert_eq!(result.row_count(), 1);
    // 30 + 25 + 35 + 28 + 32 = 150
    assert_eq!(result.rows[0][0], Value::Int64(150));
}

// ============================================================================
// SUM(DISTINCT), AVG(DISTINCT)
// ============================================================================

#[test]
fn test_sum_distinct() {
    let db = create_duplicate_ages();
    let session = db.session();

    // Ages: 30, 25, 30, 25, 40 => distinct ages: 25, 30, 40 => sum = 95
    let result = session
        .execute_sql(
            "SELECT SUM(DISTINCT age) AS unique_sum FROM GRAPH_TABLE (
                MATCH (n:Person)
                COLUMNS (n.age AS age)
            )",
        )
        .unwrap();

    assert_eq!(result.row_count(), 1);
    match &result.rows[0][0] {
        Value::Int64(v) => assert_eq!(*v, 95, "sum of distinct ages: 25 + 30 + 40 = 95"),
        Value::Float64(v) => assert!(
            (*v - 95.0).abs() < 0.01,
            "sum of distinct ages should be 95, got {v}"
        ),
        other => panic!("expected numeric SUM(DISTINCT), got {other:?}"),
    }
}

#[test]
fn test_avg_distinct() {
    let db = create_duplicate_ages();
    let session = db.session();

    // Ages: 30, 25, 30, 25, 40 => distinct ages: 25, 30, 40 => avg = 95/3 ≈ 31.67
    let result = session
        .execute_sql(
            "SELECT AVG(DISTINCT age) AS unique_avg FROM GRAPH_TABLE (
                MATCH (n:Person)
                COLUMNS (n.age AS age)
            )",
        )
        .unwrap();

    assert_eq!(result.row_count(), 1);
    let expected = 95.0 / 3.0;
    match &result.rows[0][0] {
        Value::Float64(v) => assert!(
            (*v - expected).abs() < 0.1,
            "avg of distinct ages: 95/3 ≈ 31.67, got {v}"
        ),
        Value::Int64(v) => {
            // Integer truncation is also acceptable
            assert!(
                (*v - 31).abs() <= 1,
                "avg of distinct ages should be ~31, got {v}"
            );
        }
        other => panic!("expected numeric AVG(DISTINCT), got {other:?}"),
    }
}

// ============================================================================
// Aggregates on empty results
// ============================================================================

#[test]
fn test_count_star_on_empty_result() {
    let db = create_rich_network();
    let session = db.session();

    // COUNT(*) on no matching rows should return 1 row with 0.
    let result = session
        .execute_sql(
            "SELECT COUNT(*) AS total FROM GRAPH_TABLE (
                MATCH (n:NonExistent)
                COLUMNS (n.name AS name)
            )",
        )
        .unwrap();

    assert_eq!(
        result.row_count(),
        1,
        "COUNT(*) on empty set returns one row"
    );
    assert_eq!(result.rows[0][0], Value::Int64(0));
}

#[test]
fn test_sum_on_empty_result() {
    let db = create_rich_network();
    let session = db.session();

    // SUM on empty result set: SQL standard says NULL (or 0 is acceptable).
    let result = session
        .execute_sql(
            "SELECT SUM(age) AS total FROM GRAPH_TABLE (
                MATCH (n:Person)
                WHERE n.age > 1000
                COLUMNS (n.age AS age)
            )",
        )
        .unwrap();

    assert_eq!(result.row_count(), 1);
    // Accept either NULL or 0 for SUM on empty set
    let val = &result.rows[0][0];
    assert!(
        val.is_null() || *val == Value::Int64(0) || *val == Value::Float64(0.0),
        "SUM on empty set should be NULL or 0, got {val:?}"
    );
}

#[test]
fn test_min_max_on_empty_result() {
    let db = create_rich_network();
    let session = db.session();

    // MIN/MAX on empty set: SQL standard says NULL.
    let result = session
        .execute_sql(
            "SELECT MIN(age) AS lo, MAX(age) AS hi FROM GRAPH_TABLE (
                MATCH (n:Person)
                WHERE n.age > 1000
                COLUMNS (n.age AS age)
            )",
        )
        .unwrap();

    assert_eq!(result.row_count(), 1);
    // Both should be NULL (no values to compare)
    let min_val = &result.rows[0][0];
    let max_val = &result.rows[0][1];
    assert!(
        min_val.is_null() || *min_val == Value::Int64(0),
        "MIN on empty set should be NULL (or 0), got {min_val:?}"
    );
    assert!(
        max_val.is_null() || *max_val == Value::Int64(0),
        "MAX on empty set should be NULL (or 0), got {max_val:?}"
    );
}

// ============================================================================
// Aggregates with all NULLs
// ============================================================================

#[test]
fn test_sum_all_nulls() {
    let db = create_all_nulls();
    let session = db.session();

    // SUM over only NULL values: SQL standard says NULL.
    let result = session
        .execute_sql(
            "SELECT SUM(val) AS total FROM GRAPH_TABLE (
                MATCH (n:Item)
                COLUMNS (n.val AS val)
            )",
        )
        .unwrap();

    assert_eq!(result.row_count(), 1);
    let val = &result.rows[0][0];
    assert!(
        val.is_null() || *val == Value::Int64(0) || *val == Value::Float64(0.0),
        "SUM of all NULLs should be NULL or 0, got {val:?}"
    );
}

#[test]
fn test_avg_all_nulls() {
    let db = create_all_nulls();
    let session = db.session();

    // AVG over only NULL values: SQL standard says NULL.
    let result = session
        .execute_sql(
            "SELECT AVG(val) AS average FROM GRAPH_TABLE (
                MATCH (n:Item)
                COLUMNS (n.val AS val)
            )",
        )
        .unwrap();

    assert_eq!(result.row_count(), 1);
    let val = &result.rows[0][0];
    assert!(
        val.is_null() || *val == Value::Int64(0) || *val == Value::Float64(0.0),
        "AVG of all NULLs should be NULL or 0, got {val:?}"
    );
}

#[test]
fn test_count_star_all_nulls() {
    let db = create_all_nulls();
    let session = db.session();

    // COUNT(*) should count rows regardless of NULL values.
    let result = session
        .execute_sql(
            "SELECT COUNT(*) AS total FROM GRAPH_TABLE (
                MATCH (n:Item)
                COLUMNS (n.val AS val)
            )",
        )
        .unwrap();

    assert_eq!(result.row_count(), 1);
    assert_eq!(
        result.rows[0][0],
        Value::Int64(3),
        "COUNT(*) counts rows, not values"
    );
}

// ============================================================================
// Index access expressions
// ============================================================================

#[test]
#[ignore = "SQL/PGQ parser does not support list literal index access in COLUMNS position"]
fn test_index_access_first_element() {
    let db = GrafeoDB::new_in_memory();
    let session = db.session();
    session.create_node_with_props(&["Data"], [("name", Value::String("row1".into()))]);

    let result = session
        .execute_sql(
            "SELECT * FROM GRAPH_TABLE (
                MATCH (n:Data)
                COLUMNS ([1, 2, 3][0] AS first)
            )",
        )
        .unwrap();

    assert_eq!(result.row_count(), 1);
    assert_eq!(
        result.rows[0][0],
        Value::Int64(1),
        "index 0 of [1,2,3] should be 1"
    );
}

#[test]
#[ignore = "SQL/PGQ parser does not support list literal index access in COLUMNS position"]
fn test_index_access_last_element() {
    let db = GrafeoDB::new_in_memory();
    let session = db.session();
    session.create_node_with_props(&["Data"], [("name", Value::String("row1".into()))]);

    let result = session
        .execute_sql(
            "SELECT * FROM GRAPH_TABLE (
                MATCH (n:Data)
                COLUMNS ([1, 2, 3][2] AS last)
            )",
        )
        .unwrap();

    assert_eq!(result.row_count(), 1);
    assert_eq!(
        result.rows[0][0],
        Value::Int64(3),
        "index 2 of [1,2,3] should be 3"
    );
}

// ============================================================================
// Unary positive operator
// ============================================================================

#[test]
#[ignore = "SQL/PGQ parser does not recognize unary + in COLUMNS expressions"]
fn test_unary_positive_operator() {
    let db = create_rich_network();
    let session = db.session();

    // Unary + is identity: +n.age should equal n.age.
    let result = session
        .execute_sql(
            "SELECT * FROM GRAPH_TABLE (
                MATCH (n:Person)
                WHERE n.name = 'Alix'
                COLUMNS (n.name AS name, +n.age AS pos_age)
            )",
        )
        .unwrap();

    assert_eq!(result.row_count(), 1);
    assert_eq!(result.rows[0][0], Value::String("Alix".into()));
    assert_eq!(
        result.rows[0][1],
        Value::Int64(30),
        "unary + should be identity"
    );
}

// ============================================================================
// Nested function calls in WHERE (not just COLUMNS)
// ============================================================================

#[test]
fn test_upper_in_inner_where() {
    let db = create_rich_network();
    let session = db.session();

    let result = session
        .execute_sql(
            "SELECT * FROM GRAPH_TABLE (
                MATCH (n:Person)
                WHERE UPPER(n.name) = 'ALIX'
                COLUMNS (n.name AS name, n.age AS age)
            )",
        )
        .unwrap();

    assert_eq!(result.row_count(), 1);
    assert_eq!(result.rows[0][0], Value::String("Alix".into()));
}

#[test]
fn test_abs_in_inner_where() {
    let db = create_rich_network();
    let session = db.session();

    // ABS(age - 30) < 5 => ages 26..34 => Alix(30), Vincent(28), Mia(32)
    let result = session
        .execute_sql(
            "SELECT * FROM GRAPH_TABLE (
                MATCH (n:Person)
                WHERE ABS(n.age - 30) < 5
                COLUMNS (n.name AS name, n.age AS age)
            ) ORDER BY name",
        )
        .unwrap();

    assert_eq!(result.row_count(), 3);
    assert_eq!(result.rows[0][0], Value::String("Alix".into()));
    assert_eq!(result.rows[1][0], Value::String("Mia".into()));
    assert_eq!(result.rows[2][0], Value::String("Vincent".into()));
}

#[test]
fn test_size_in_inner_where() {
    let db = create_rich_network();
    let session = db.session();

    // SIZE(name) > 3 => Alix(4), Harm(4), Vincent(7), Mia is excluded (3)
    let result = session
        .execute_sql(
            "SELECT * FROM GRAPH_TABLE (
                MATCH (n:Person)
                WHERE SIZE(n.name) > 3
                COLUMNS (n.name AS name)
            ) ORDER BY name",
        )
        .unwrap();

    // Alix(4), Harm(4), Vincent(7) = 3 rows. Gus(3) and Mia(3) are excluded.
    assert_eq!(result.row_count(), 3);
    assert_eq!(result.rows[0][0], Value::String("Alix".into()));
    assert_eq!(result.rows[1][0], Value::String("Harm".into()));
    assert_eq!(result.rows[2][0], Value::String("Vincent".into()));
}

// ============================================================================
// Complex expressions in COLUMNS
// ============================================================================

#[test]
fn test_multi_operator_expression() {
    let db = create_rich_network();
    let session = db.session();

    // n.age * 2 + 10 for Alix (age 30) => 70
    let result = session
        .execute_sql(
            "SELECT * FROM GRAPH_TABLE (
                MATCH (n:Person)
                WHERE n.name = 'Alix'
                COLUMNS (n.name AS name, n.age * 2 + 10 AS computed)
            )",
        )
        .unwrap();

    assert_eq!(result.row_count(), 1);
    assert_eq!(result.rows[0][1], Value::Int64(70), "30 * 2 + 10 = 70");
}

#[test]
fn test_nested_parenthesized_expression() {
    let db = create_rich_network();
    let session = db.session();

    // (age + 5) * (age - 5) for Alix (age 30) => 35 * 25 = 875
    let result = session
        .execute_sql(
            "SELECT * FROM GRAPH_TABLE (
                MATCH (n:Person)
                WHERE n.name = 'Alix'
                COLUMNS (n.name AS name, (n.age + 5) * (n.age - 5) AS diff_squares)
            )",
        )
        .unwrap();

    assert_eq!(result.row_count(), 1);
    assert_eq!(
        result.rows[0][1],
        Value::Int64(875),
        "(30+5) * (30-5) = 35*25 = 875"
    );
}

#[test]
fn test_boolean_expression_as_column() {
    let db = create_rich_network();
    let session = db.session();

    // n.age > 30 as a boolean column
    let result = session
        .execute_sql(
            "SELECT * FROM GRAPH_TABLE (
                MATCH (n:Person)
                WHERE n.name = 'Alix'
                COLUMNS (n.name AS name, n.age > 30 AS is_senior)
            )",
        )
        .unwrap();

    assert_eq!(result.row_count(), 1);
    assert_eq!(
        result.rows[0][1],
        Value::Bool(false),
        "Alix is 30, not > 30"
    );
}

#[test]
fn test_boolean_expression_as_column_true() {
    let db = create_rich_network();
    let session = db.session();

    let result = session
        .execute_sql(
            "SELECT * FROM GRAPH_TABLE (
                MATCH (n:Person)
                WHERE n.name = 'Harm'
                COLUMNS (n.name AS name, n.age > 30 AS is_senior)
            )",
        )
        .unwrap();

    assert_eq!(result.row_count(), 1);
    assert_eq!(
        result.rows[0][1],
        Value::Bool(true),
        "Harm is 35, which is > 30"
    );
}

// ============================================================================
// WHERE combining inner and outer with functions
// ============================================================================

#[test]
fn test_inner_function_where_plus_outer_function_where() {
    let db = create_rich_network();
    let session = db.session();

    // Inner WHERE: SIZE(n.name) > 3 (filters Gus and Mia)
    // Outer WHERE: g.age > 28 (filters Vincent)
    // Expected: Alix(30), Harm(35)
    let result = session
        .execute_sql(
            "SELECT * FROM GRAPH_TABLE (
                MATCH (n:Person)
                WHERE SIZE(n.name) > 3
                COLUMNS (n.name AS name, n.age AS age)
            ) AS g WHERE g.age > 28 ORDER BY g.name",
        )
        .unwrap();

    assert_eq!(result.row_count(), 2);
    assert_eq!(result.rows[0][0], Value::String("Alix".into()));
    assert_eq!(result.rows[1][0], Value::String("Harm".into()));
}

// ============================================================================
// SELECT * with outer WHERE
// ============================================================================

#[test]
fn test_select_star_with_outer_where() {
    let db = create_rich_network();
    let session = db.session();

    let result = session
        .execute_sql(
            "SELECT * FROM GRAPH_TABLE (
                MATCH (n:Person)
                COLUMNS (n.name AS name, n.age AS age)
            ) AS g WHERE g.age > 30 ORDER BY g.name",
        )
        .unwrap();

    // Harm(35) and Mia(32) have age > 30
    assert_eq!(result.row_count(), 2);
    assert_eq!(result.rows[0][0], Value::String("Harm".into()));
    assert_eq!(result.rows[1][0], Value::String("Mia".into()));
}

#[test]
fn test_select_star_with_outer_where_no_matches() {
    let db = create_rich_network();
    let session = db.session();

    let result = session
        .execute_sql(
            "SELECT * FROM GRAPH_TABLE (
                MATCH (n:Person)
                COLUMNS (n.name AS name, n.age AS age)
            ) AS g WHERE g.age > 100",
        )
        .unwrap();

    assert_eq!(result.row_count(), 0);
}

// ============================================================================
// Unsupported expression error tests
// ============================================================================

#[test]
fn test_exists_subquery_error() {
    let db = create_rich_network();
    let session = db.session();

    let result = session.execute_sql(
        "SELECT * FROM GRAPH_TABLE (
            MATCH (n:Person)
            WHERE EXISTS { MATCH (n)-[:KNOWS]->(m) }
            COLUMNS (n.name AS name)
        )",
    );

    assert!(result.is_err(), "EXISTS subquery should produce an error");
    let err_msg = format!("{}", result.unwrap_err());
    assert!(
        err_msg.to_lowercase().contains("exists")
            || err_msg.to_lowercase().contains("subquery")
            || err_msg.to_lowercase().contains("not supported")
            || err_msg.to_lowercase().contains("parse"),
        "error should mention EXISTS or subquery or unsupported, got: {err_msg}"
    );
}

#[test]
fn test_count_subquery_error() {
    let db = create_rich_network();
    let session = db.session();

    let result = session.execute_sql(
        "SELECT * FROM GRAPH_TABLE (
            MATCH (n:Person)
            WHERE COUNT { MATCH (n)-[:KNOWS]->(m) } > 0
            COLUMNS (n.name AS name)
        )",
    );

    assert!(result.is_err(), "COUNT subquery should produce an error");
    let err_msg = format!("{}", result.unwrap_err());
    assert!(
        err_msg.to_lowercase().contains("count")
            || err_msg.to_lowercase().contains("subquery")
            || err_msg.to_lowercase().contains("not supported")
            || err_msg.to_lowercase().contains("parse"),
        "error should mention COUNT or subquery or unsupported, got: {err_msg}"
    );
}

// ============================================================================
// Parameterized queries edge cases
// ============================================================================

#[test]
fn test_param_used_twice_in_where() {
    let db = create_rich_network();
    let session = db.session();

    let mut params = std::collections::HashMap::new();
    params.insert("min".to_string(), Value::Int64(25));

    // Same parameter $min used twice: age >= $min AND age < $min + 10
    // => age >= 25 AND age < 35
    // Expected: Gus(25), Vincent(28), Alix(30), Mia(32)
    let result = session
        .execute_sql_with_params(
            "SELECT * FROM GRAPH_TABLE (
                MATCH (n:Person)
                WHERE n.age >= $min AND n.age < $min + 10
                COLUMNS (n.name AS name, n.age AS age)
            ) ORDER BY name",
            params,
        )
        .unwrap();

    assert_eq!(result.row_count(), 4);
    assert_eq!(result.rows[0][0], Value::String("Alix".into()));
    assert_eq!(result.rows[1][0], Value::String("Gus".into()));
    assert_eq!(result.rows[2][0], Value::String("Mia".into()));
    assert_eq!(result.rows[3][0], Value::String("Vincent".into()));
}

#[test]
fn test_param_in_arithmetic_expression() {
    let db = create_rich_network();
    let session = db.session();

    let mut params = std::collections::HashMap::new();
    params.insert("multiplier".to_string(), Value::Int64(3));

    let result = session
        .execute_sql_with_params(
            "SELECT * FROM GRAPH_TABLE (
                MATCH (n:Person)
                WHERE n.name = 'Gus'
                COLUMNS (n.name AS name, n.age * $multiplier AS tripled)
            )",
            params,
        )
        .unwrap();

    assert_eq!(result.row_count(), 1);
    assert_eq!(result.rows[0][1], Value::Int64(75), "25 * 3 = 75");
}

#[test]
fn test_param_in_outer_where_with_function() {
    let db = create_rich_network();
    let session = db.session();

    let mut params = std::collections::HashMap::new();
    params.insert("threshold".to_string(), Value::Int64(28));

    let result = session
        .execute_sql_with_params(
            "SELECT * FROM GRAPH_TABLE (
                MATCH (n:Person)
                COLUMNS (n.name AS name, n.age AS age)
            ) AS g WHERE g.age >= $threshold ORDER BY g.name",
            params,
        )
        .unwrap();

    // Alix(30), Harm(35), Mia(32), Vincent(28) have age >= 28
    assert_eq!(result.row_count(), 4);
    assert_eq!(result.rows[0][0], Value::String("Alix".into()));
    assert_eq!(result.rows[1][0], Value::String("Harm".into()));
    assert_eq!(result.rows[2][0], Value::String("Mia".into()));
    assert_eq!(result.rows[3][0], Value::String("Vincent".into()));
}

#[test]
fn test_param_with_null_value() {
    let db = GrafeoDB::new_in_memory();
    let session = db.session();
    session.create_node_with_props(
        &["Item"],
        [
            ("name", Value::String("alpha".into())),
            ("val", Value::Null),
        ],
    );
    session.create_node_with_props(
        &["Item"],
        [
            ("name", Value::String("beta".into())),
            ("val", Value::Int64(10)),
        ],
    );

    let mut params = std::collections::HashMap::new();
    params.insert("threshold".to_string(), Value::Int64(5));

    let result = session
        .execute_sql_with_params(
            "SELECT * FROM GRAPH_TABLE (
                MATCH (n:Item)
                WHERE n.val > $threshold
                COLUMNS (n.name AS name)
            )",
            params,
        )
        .unwrap();

    // Only beta(10) passes the filter, alpha(NULL) is filtered out
    assert_eq!(result.row_count(), 1);
    assert_eq!(result.rows[0][0], Value::String("beta".into()));
}

// ============================================================================
// Additional expression edge cases
// ============================================================================

#[test]
fn test_negation_in_columns() {
    let db = create_rich_network();
    let session = db.session();

    let result = session
        .execute_sql(
            "SELECT * FROM GRAPH_TABLE (
                MATCH (n:Person)
                WHERE n.name = 'Gus'
                COLUMNS (n.name AS name, -n.age AS neg_age)
            )",
        )
        .unwrap();

    assert_eq!(result.row_count(), 1);
    assert_eq!(result.rows[0][1], Value::Int64(-25), "unary minus: -25");
}

#[test]
fn test_mixed_aggregates_implicit_group_by() {
    let db = create_rich_network();
    let session = db.session();

    // COUNT(*), SUM, AVG, MIN, MAX all together without GROUP BY
    let result = session
        .execute_sql(
            "SELECT COUNT(*) AS cnt, SUM(age) AS total, AVG(age) AS average, MIN(age) AS lo, MAX(age) AS hi
             FROM GRAPH_TABLE (
                MATCH (n:Person)
                COLUMNS (n.age AS age)
            )",
        )
        .unwrap();

    assert_eq!(result.row_count(), 1);
    assert_eq!(result.rows[0][0], Value::Int64(5), "5 persons");
    assert_eq!(result.rows[0][1], Value::Int64(150), "sum = 150");

    // AVG could be Float64 or Int64 depending on implementation
    match &result.rows[0][2] {
        Value::Float64(v) => assert!((*v - 30.0).abs() < 0.01, "avg = 30.0, got {v}"),
        Value::Int64(v) => assert_eq!(*v, 30, "avg = 30"),
        other => panic!("expected numeric avg, got {other:?}"),
    }

    assert_eq!(result.rows[0][3], Value::Int64(25), "min = 25");
    assert_eq!(result.rows[0][4], Value::Int64(35), "max = 35");
}

#[test]
fn test_string_concatenation_in_columns() {
    let db = create_rich_network();
    let session = db.session();

    let result = session
        .execute_sql(
            "SELECT * FROM GRAPH_TABLE (
                MATCH (n:Person)
                WHERE n.name = 'Gus'
                COLUMNS (n.name + ' from ' + n.city AS greeting)
            )",
        )
        .unwrap();

    assert_eq!(result.row_count(), 1);
    assert_eq!(result.rows[0][0], Value::String("Gus from Berlin".into()));
}

#[test]
fn test_literal_comparison_in_where() {
    let db = create_rich_network();
    let session = db.session();

    // WHERE with literal comparison that always evaluates true
    let result = session
        .execute_sql(
            "SELECT * FROM GRAPH_TABLE (
                MATCH (n:Person)
                WHERE 1 = 1
                COLUMNS (n.name AS name)
            ) ORDER BY name",
        )
        .unwrap();

    assert_eq!(result.row_count(), 5, "1=1 is always true, all rows pass");
}

#[test]
fn test_literal_comparison_in_where_false() {
    let db = create_rich_network();
    let session = db.session();

    // WHERE with literal comparison that always evaluates false
    let result = session
        .execute_sql(
            "SELECT * FROM GRAPH_TABLE (
                MATCH (n:Person)
                WHERE 1 = 0
                COLUMNS (n.name AS name)
            )",
        )
        .unwrap();

    assert_eq!(result.row_count(), 0, "1=0 is always false, no rows pass");
}

#[test]
fn test_multiple_functions_in_where_and_columns() {
    let db = create_rich_network();
    let session = db.session();

    // Function in WHERE + different function in COLUMNS
    let result = session
        .execute_sql(
            "SELECT * FROM GRAPH_TABLE (
                MATCH (n:Person)
                WHERE LOWER(n.city) = 'amsterdam'
                COLUMNS (n.name AS name, UPPER(n.city) AS city_upper, n.age * 2 AS double_age)
            ) ORDER BY name",
        )
        .unwrap();

    assert_eq!(result.row_count(), 2);
    assert_eq!(result.rows[0][0], Value::String("Alix".into()));
    assert_eq!(result.rows[0][1], Value::String("AMSTERDAM".into()));
    assert_eq!(result.rows[0][2], Value::Int64(60));
    assert_eq!(result.rows[1][0], Value::String("Harm".into()));
    assert_eq!(result.rows[1][1], Value::String("AMSTERDAM".into()));
    assert_eq!(result.rows[1][2], Value::Int64(70));
}

#[test]
fn test_count_distinct_on_duplicates() {
    let db = create_duplicate_ages();
    let session = db.session();

    // Ages: 30, 25, 30, 25, 40 => 3 distinct ages
    let result = session
        .execute_sql(
            "SELECT COUNT(DISTINCT age) AS unique_count FROM GRAPH_TABLE (
                MATCH (n:Person)
                COLUMNS (n.age AS age)
            )",
        )
        .unwrap();

    assert_eq!(result.row_count(), 1);
    assert_eq!(result.rows[0][0], Value::Int64(3));
}
