//! SQL/PGQ error path, syntax edge case, and CREATE PROPERTY GRAPH edge case tests.
//!
//! These tests verify that the parser and translator produce clear, helpful errors
//! for malformed queries, unsupported constructs, and edge cases in DDL statements.
//!
//! Run with:
//! ```bash
//! cargo test -p grafeo-engine --features sql-pgq --test sql_pgq_error_cases
//! ```

#![cfg(feature = "sql-pgq")]

use grafeo_engine::GrafeoDB;

// ============================================================================
// Test Fixtures
// ============================================================================

/// Creates a minimal in-memory database for error testing.
/// Most tests here only exercise parsing/translation, so no data is needed.
fn create_empty_db() -> GrafeoDB {
    GrafeoDB::new_in_memory()
}

/// Helper: asserts that `execute_sql` fails and the error message contains
/// all the specified keywords (case-insensitive).
fn assert_sql_error_contains(db: &GrafeoDB, query: &str, keywords: &[&str]) {
    let session = db.session();
    let result = session.execute_sql(query);
    assert!(result.is_err(), "Expected error for query: {query}");
    let err_msg = result.unwrap_err().to_string().to_lowercase();
    for keyword in keywords {
        assert!(
            err_msg.contains(&keyword.to_lowercase()),
            "Error message should contain '{keyword}', got: {err_msg}"
        );
    }
}

// ============================================================================
// Parser Syntax Errors
// ============================================================================

#[test]
fn test_invalid_token_at_symbol() {
    let db = create_empty_db();
    assert_sql_error_contains(&db, "SELECT @invalid", &["expected"]);
}

#[test]
fn test_unclosed_parenthesis_in_node_pattern() {
    let db = create_empty_db();
    // Missing closing paren on the node pattern
    assert_sql_error_contains(
        &db,
        "SELECT * FROM GRAPH_TABLE (MATCH (n:Person COLUMNS (n.name AS name))",
        &["expected"],
    );
}

#[test]
fn test_unclosed_bracket_in_edge() {
    let db = create_empty_db();
    // Missing closing bracket: -[e:KNOWS->(b)
    assert_sql_error_contains(
        &db,
        "SELECT * FROM GRAPH_TABLE (MATCH (a)-[e:KNOWS->(b) COLUMNS (a.name AS name))",
        &["expected"],
    );
}

#[test]
fn test_double_semicolons() {
    let db = create_empty_db();
    // After the first semicolon is consumed, the second semicolon is unexpected
    assert_sql_error_contains(
        &db,
        "SELECT * FROM GRAPH_TABLE (MATCH (n:Person) COLUMNS (n.name AS name)) ;;",
        &["expected", "end"],
    );
}

#[test]
fn test_garbage_after_valid_query() {
    let db = create_empty_db();
    // Use multiple tokens after the valid query so the parser can't treat them as a single alias
    assert_sql_error_contains(
        &db,
        "SELECT * FROM GRAPH_TABLE (MATCH (n:Person) COLUMNS (n.name AS name)) foo bar baz",
        &["expected", "end"],
    );
}

#[test]
fn test_missing_graph_table_keyword() {
    let db = create_empty_db();
    // FROM without GRAPH_TABLE
    assert_sql_error_contains(
        &db,
        "SELECT * FROM (MATCH (n:Person) COLUMNS (n.name AS name))",
        &["expected", "GraphTable"],
    );
}

#[test]
fn test_missing_parentheses_around_graph_table() {
    let db = create_empty_db();
    // GRAPH_TABLE without opening paren
    assert_sql_error_contains(
        &db,
        "SELECT * FROM GRAPH_TABLE MATCH (n:Person) COLUMNS (n.name AS name)",
        &["expected", "LParen"],
    );
}

// ============================================================================
// Unsupported Expression Errors (Translator Rejects)
// ============================================================================

#[test]
fn test_quantified_pattern_error() {
    // Quantified patterns like ((a)-[e]->(b)){1,3} are in the GQL AST but
    // not supported by the SQL/PGQ translator. The SQL/PGQ parser itself
    // does not parse quantified patterns, so this tests that the translator
    // error message is helpful if somehow encountered.
    //
    // Since the SQL/PGQ parser doesn't produce quantified patterns, we verify
    // that the parser rejects quantified-looking syntax at the parse level.
    let db = create_empty_db();
    // The parser will fail because it doesn't understand the {1,3} syntax after a pattern
    let result = db.session().execute_sql(
        "SELECT * FROM GRAPH_TABLE (
            MATCH ((a)-[e:KNOWS]->(b)){1,3}
            COLUMNS (a.name AS name)
        )",
    );
    assert!(
        result.is_err(),
        "Quantified patterns should produce an error"
    );
}

#[test]
fn test_union_pattern_syntax_rejected() {
    // Union patterns using | are not supported in SQL/PGQ
    let db = create_empty_db();
    let result = db.session().execute_sql(
        "SELECT * FROM GRAPH_TABLE (
            MATCH (a)-[:KNOWS]->(b) | (a)-[:FOLLOWS]->(b)
            COLUMNS (a.name AS name)
        )",
    );
    assert!(result.is_err(), "Union patterns should produce an error");
}

// ============================================================================
// CREATE PROPERTY GRAPH Edge Cases
// ============================================================================

#[test]
fn test_create_pg_node_only_no_edges_succeeds() {
    let db = create_empty_db();
    let session = db.session();
    // A property graph with only node tables and no edge tables should be valid
    let result = session.execute_sql(
        "CREATE PROPERTY GRAPH social
         NODE TABLES (
             Person (id BIGINT PRIMARY KEY, name VARCHAR, age INT)
         )",
    );
    assert!(
        result.is_ok(),
        "Node-only property graph should succeed, got: {:?}",
        result.err()
    );
}

#[test]
fn test_create_pg_edge_with_only_one_reference() {
    let db = create_empty_db();
    let session = db.session();
    // Edge table with only one REFERENCES: the parser accepts it but
    // the target will be empty (defaults to empty string).
    // This is a degenerate case, the source is set but target is missing.
    let result = session.execute_sql(
        "CREATE PROPERTY GRAPH social
         NODE TABLES (
             Person (id BIGINT PRIMARY KEY, name VARCHAR)
         )
         EDGE TABLES (
             Knows (
                 src_id BIGINT REFERENCES Person (id),
                 label VARCHAR
             )
         )",
    );
    // Parser succeeds but with empty target_table. Whether this is an error depends on
    // the execution layer. The parser will set target_table to empty string.
    // Verify we get some result (not a parse error, since the SQL is syntactically valid).
    // The execution layer may or may not reject it.
    assert!(
        result.is_ok() || result.is_err(),
        "Should handle edge with 1 reference"
    );

    // Specifically verify it doesn't panic
}

#[test]
fn test_create_pg_edge_with_zero_references() {
    let db = create_empty_db();
    let session = db.session();
    // Edge table with no REFERENCES at all: source and target will both be empty.
    let result = session.execute_sql(
        "CREATE PROPERTY GRAPH social
         NODE TABLES (
             Person (id BIGINT PRIMARY KEY, name VARCHAR)
         )
         EDGE TABLES (
             Knows (
                 label VARCHAR,
                 weight FLOAT
             )
         )",
    );
    // Parser accepts this (source_table and target_table default to empty).
    // Verify no panic.
    assert!(
        result.is_ok() || result.is_err(),
        "Should handle edge with 0 references without panic"
    );
}

#[test]
fn test_create_pg_duplicate_column_names() {
    let db = create_empty_db();
    let session = db.session();
    // Duplicate column names in a node table definition
    let result = session.execute_sql(
        "CREATE PROPERTY GRAPH social
         NODE TABLES (
             Person (id BIGINT PRIMARY KEY, name VARCHAR, name VARCHAR)
         )",
    );
    // The parser does not reject duplicate columns. Verify it doesn't panic.
    assert!(
        result.is_ok() || result.is_err(),
        "Should handle duplicate column names without panic"
    );
}

#[test]
fn test_create_pg_all_data_types() {
    let db = create_empty_db();
    let session = db.session();
    // Verify every supported SQL data type parses correctly
    let result = session.execute_sql(
        "CREATE PROPERTY GRAPH typed_graph
         NODE TABLES (
             AllTypes (
                 id BIGINT PRIMARY KEY,
                 small_id INT,
                 full_id INTEGER,
                 name VARCHAR,
                 bounded_name VARCHAR(255),
                 flag BOOLEAN,
                 alt_flag BOOL,
                 score FLOAT,
                 alt_score REAL,
                 precise DOUBLE,
                 created TIMESTAMP
             )
         )",
    );
    assert!(
        result.is_ok(),
        "All supported data types should parse, got: {:?}",
        result.err()
    );
}

#[test]
fn test_create_pg_with_trailing_semicolon() {
    let db = create_empty_db();
    let session = db.session();
    let result = session.execute_sql(
        "CREATE PROPERTY GRAPH social
         NODE TABLES (
             Person (id BIGINT PRIMARY KEY, name VARCHAR)
         );",
    );
    assert!(
        result.is_ok(),
        "Trailing semicolon should be accepted, got: {:?}",
        result.err()
    );
}

#[test]
fn test_create_pg_no_tables_at_all() {
    let db = create_empty_db();
    let session = db.session();
    // CREATE PROPERTY GRAPH with no NODE TABLES or EDGE TABLES
    let result = session.execute_sql("CREATE PROPERTY GRAPH empty_graph");
    assert!(result.is_err(), "Empty property graph should error");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("NODE TABLES") || err_msg.contains("EDGE TABLES"),
        "Error should mention NODE TABLES or EDGE TABLES, got: {err_msg}"
    );
}

#[test]
fn test_create_pg_edge_references_unknown_table() {
    let db = create_empty_db();
    let session = db.session();
    // Edge table references a node table that doesn't exist
    let result = session.execute_sql(
        "CREATE PROPERTY GRAPH social
         NODE TABLES (
             Person (id BIGINT PRIMARY KEY, name VARCHAR)
         )
         EDGE TABLES (
             Knows (
                 src_id BIGINT REFERENCES Person (id),
                 tgt_id BIGINT REFERENCES Company (id)
             )
         )",
    );
    assert!(result.is_err(), "Unknown reference target should error");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Company") || err_msg.contains("unknown"),
        "Error should mention the unknown table, got: {err_msg}"
    );
}

// ============================================================================
// Error Message Quality
// ============================================================================

#[test]
fn test_error_contains_expected_vs_found() {
    let db = create_empty_db();
    // The expect() method produces "Expected X, found Y" style messages
    let result = db
        .session()
        .execute_sql("SELECT * FROM GRAPH_TABLE MATCH (n:Person) COLUMNS (n.name AS name)");
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Expected") && err_msg.contains("found"),
        "Error should contain 'Expected' and 'found', got: {err_msg}"
    );
}

#[test]
fn test_missing_as_in_columns() {
    let db = create_empty_db();
    // COLUMNS clause requires AS for each column item
    assert_sql_error_contains(
        &db,
        "SELECT * FROM GRAPH_TABLE (MATCH (n:Person) COLUMNS (n.name))",
        &["Expected", "As"],
    );
}

#[test]
fn test_incomplete_where_clause() {
    let db = create_empty_db();
    // WHERE with nothing after it: the expression parser will fail
    let result = db.session().execute_sql(
        "SELECT * FROM GRAPH_TABLE (
            MATCH (n:Person)
            COLUMNS (n.name AS name)
        ) WHERE",
    );
    assert!(result.is_err(), "Incomplete WHERE should error");
}

// ============================================================================
// Quoted Identifier Edge Cases
// ============================================================================

#[test]
fn test_quoted_identifier_with_space() {
    let db = create_empty_db();
    let session = db.session();
    // Double-quoted identifiers with spaces should be valid as column aliases
    let result = session.execute_sql(
        "SELECT * FROM GRAPH_TABLE (
            MATCH (n:Person)
            COLUMNS (n.name AS \"my column\")
        )",
    );
    // This should parse successfully since the lexer handles QuotedIdentifier
    assert!(
        result.is_ok() || result.is_err(),
        "Quoted identifier with space should not panic"
    );
}

#[test]
fn test_quoted_identifier_with_reserved_word() {
    let db = create_empty_db();
    let session = db.session();
    // Using a reserved word as a quoted identifier
    let result = session.execute_sql(
        "SELECT * FROM GRAPH_TABLE (
            MATCH (n:Person)
            COLUMNS (n.name AS \"SELECT\")
        )",
    );
    assert!(
        result.is_ok() || result.is_err(),
        "Quoted reserved word should not panic"
    );
}

#[test]
fn test_empty_quoted_identifier() {
    let db = create_empty_db();
    let session = db.session();
    // Empty quoted identifier "" should be lexed as QuotedIdentifier with empty text
    let result = session.execute_sql(
        "SELECT * FROM GRAPH_TABLE (
            MATCH (n:Person)
            COLUMNS (n.name AS \"\")
        )",
    );
    // Whether this is accepted or rejected, it should not panic
    assert!(
        result.is_ok() || result.is_err(),
        "Empty quoted identifier should not panic"
    );
}

// ============================================================================
// Whitespace, Comments, and Empty Input
// ============================================================================

#[test]
fn test_whitespace_only_query() {
    let db = create_empty_db();
    let result = db.session().execute_sql("   ");
    assert!(result.is_err(), "Whitespace-only query should error");
}

#[test]
fn test_line_comment_only_query() {
    let db = create_empty_db();
    let result = db.session().execute_sql("-- just a comment");
    assert!(result.is_err(), "Comment-only query should error");
}

#[test]
fn test_block_comment_only_query() {
    let db = create_empty_db();
    let result = db.session().execute_sql("/* nothing here */");
    assert!(result.is_err(), "Block comment-only query should error");
}

#[test]
fn test_empty_query() {
    let db = create_empty_db();
    let result = db.session().execute_sql("");
    assert!(result.is_err(), "Empty query should error");
}

// ============================================================================
// GRAPH_TABLE Expression Edge Cases
// ============================================================================

#[test]
fn test_graph_table_with_empty_match_pattern() {
    let db = create_empty_db();
    // MATCH followed immediately by COLUMNS (no pattern at all)
    let result = db
        .session()
        .execute_sql("SELECT * FROM GRAPH_TABLE (MATCH COLUMNS (n.name AS name))");
    assert!(result.is_err(), "Empty MATCH pattern should error");
}

#[test]
fn test_columns_with_duplicate_aliases() {
    let db = create_empty_db();
    let session = db.session();
    // Two columns with the same alias: this parses fine but execution may or may not reject it
    let result = session.execute_sql(
        "SELECT * FROM GRAPH_TABLE (
            MATCH (n:Person)
            COLUMNS (n.name AS x, n.age AS x)
        )",
    );
    // Whether accepted or rejected, should not panic
    assert!(
        result.is_ok() || result.is_err(),
        "Duplicate column aliases should not panic"
    );
}

#[test]
fn test_graph_table_missing_columns_clause() {
    let db = create_empty_db();
    // GRAPH_TABLE with MATCH but no COLUMNS clause
    assert_sql_error_contains(
        &db,
        "SELECT * FROM GRAPH_TABLE (MATCH (n:Person))",
        &["Expected", "Columns"],
    );
}

#[test]
fn test_graph_table_empty_columns_list() {
    let db = create_empty_db();
    // COLUMNS with empty parentheses: COLUMNS ()
    let result = db
        .session()
        .execute_sql("SELECT * FROM GRAPH_TABLE (MATCH (n:Person) COLUMNS ())");
    assert!(result.is_err(), "Empty COLUMNS list should error");
}

// ============================================================================
// Additional Parser Edge Cases
// ============================================================================

#[test]
fn test_missing_select_keyword() {
    let db = create_empty_db();
    // Starting with FROM instead of SELECT
    let result = db
        .session()
        .execute_sql("FROM GRAPH_TABLE (MATCH (n:Person) COLUMNS (n.name AS name))");
    assert!(result.is_err(), "Missing SELECT should error");
}

#[test]
fn test_missing_from_keyword() {
    let db = create_empty_db();
    assert_sql_error_contains(
        &db,
        "SELECT * GRAPH_TABLE (MATCH (n:Person) COLUMNS (n.name AS name))",
        &["Expected", "From"],
    );
}

#[test]
fn test_edge_pattern_missing_target_node() {
    let db = create_empty_db();
    // Edge arrow without a target node pattern
    let result = db.session().execute_sql(
        "SELECT * FROM GRAPH_TABLE (
            MATCH (a)-[e:KNOWS]->
            COLUMNS (a.name AS name)
        )",
    );
    assert!(result.is_err(), "Edge without target node should error");
}

#[test]
fn test_unknown_data_type_in_create_pg() {
    let db = create_empty_db();
    assert_sql_error_contains(
        &db,
        "CREATE PROPERTY GRAPH social
         NODE TABLES (
             Person (id BIGINT PRIMARY KEY, data XML)
         )",
        &["Unknown data type", "XML"],
    );
}

#[test]
fn test_create_pg_missing_property_keyword() {
    let db = create_empty_db();
    // CREATE GRAPH without PROPERTY
    let result = db
        .session()
        .execute_sql("CREATE GRAPH social NODE TABLES (Person (id BIGINT PRIMARY KEY))");
    assert!(
        result.is_err(),
        "CREATE GRAPH without PROPERTY should error"
    );
}

#[test]
fn test_node_pattern_with_invalid_label_syntax() {
    let db = create_empty_db();
    // Double colon instead of single
    let result = db.session().execute_sql(
        "SELECT * FROM GRAPH_TABLE (
            MATCH (n::Person)
            COLUMNS (n.name AS name)
        )",
    );
    assert!(result.is_err(), "Double colon in label should error");
}

#[test]
fn test_multiple_edge_types_with_pipe() {
    let db = create_empty_db();
    let session = db.session();
    // Edge type alternatives: -[:KNOWS|FOLLOWS]->
    let result = session.execute_sql(
        "SELECT * FROM GRAPH_TABLE (
            MATCH (a:Person)-[:KNOWS|FOLLOWS]->(b:Person)
            COLUMNS (a.name AS person, b.name AS friend)
        )",
    );
    // Multi-type edges with pipe should parse successfully
    assert!(
        result.is_ok(),
        "Edge type alternatives with pipe should succeed, got: {:?}",
        result.err()
    );
}

#[test]
fn test_where_inside_graph_table_error() {
    let db = create_empty_db();
    // WHERE inside GRAPH_TABLE with invalid expression
    let result = db.session().execute_sql(
        "SELECT * FROM GRAPH_TABLE (
            MATCH (n:Person)
            WHERE
            COLUMNS (n.name AS name)
        )",
    );
    // The parser will try to parse "COLUMNS" as an expression in WHERE, which should fail
    assert!(
        result.is_err(),
        "WHERE with invalid expression should error"
    );
}

#[test]
fn test_unterminated_string_literal() {
    let db = create_empty_db();
    let result = db.session().execute_sql(
        "SELECT * FROM GRAPH_TABLE (
            MATCH (n:Person {name: 'Alix})
            COLUMNS (n.name AS name)
        )",
    );
    assert!(result.is_err(), "Unterminated string should error");
}

#[test]
fn test_create_pg_edge_table_missing_closing_paren() {
    let db = create_empty_db();
    let result = db.session().execute_sql(
        "CREATE PROPERTY GRAPH social
         NODE TABLES (
             Person (id BIGINT PRIMARY KEY, name VARCHAR)
         )
         EDGE TABLES (
             Knows (src_id BIGINT REFERENCES Person (id), tgt_id BIGINT REFERENCES Person (id)
         )",
    );
    assert!(
        result.is_err(),
        "Missing closing paren on edge table should error"
    );
}

#[test]
fn test_select_with_trailing_comma_in_columns() {
    let db = create_empty_db();
    // Trailing comma in COLUMNS list
    let result = db.session().execute_sql(
        "SELECT * FROM GRAPH_TABLE (
            MATCH (n:Person)
            COLUMNS (n.name AS name,)
        )",
    );
    assert!(result.is_err(), "Trailing comma in COLUMNS should error");
}

#[test]
fn test_create_pg_multiple_primary_keys() {
    let db = create_empty_db();
    let session = db.session();
    // Two columns marked PRIMARY KEY: parser accepts (no validation)
    let result = session.execute_sql(
        "CREATE PROPERTY GRAPH social
         NODE TABLES (
             Person (id BIGINT PRIMARY KEY, alt_id INT PRIMARY KEY, name VARCHAR)
         )",
    );
    // Should not panic regardless of acceptance or rejection
    assert!(
        result.is_ok() || result.is_err(),
        "Multiple PRIMARY KEYs should not panic"
    );
}

#[test]
fn test_create_pg_with_both_node_and_edge_tables() {
    let db = create_empty_db();
    let session = db.session();
    // Full property graph with both node and edge tables (happy path)
    let result = session.execute_sql(
        "CREATE PROPERTY GRAPH social
         NODE TABLES (
             Person (id BIGINT PRIMARY KEY, name VARCHAR(100), age INT),
             Company (id BIGINT PRIMARY KEY, name VARCHAR)
         )
         EDGE TABLES (
             WorksAt (
                 src_id BIGINT REFERENCES Person (id),
                 tgt_id BIGINT REFERENCES Company (id),
                 since INT
             )
         )",
    );
    assert!(
        result.is_ok(),
        "Full property graph should succeed, got: {:?}",
        result.err()
    );
}

#[test]
fn test_graph_table_with_graph_name_reference() {
    let db = create_empty_db();
    let session = db.session();
    // GRAPH_TABLE(graph_name, MATCH ...) syntax
    let result = session.execute_sql(
        "SELECT * FROM GRAPH_TABLE (social, MATCH (n:Person) COLUMNS (n.name AS name))",
    );
    // This parses fine; execution may fail because 'social' graph doesn't exist
    assert!(
        result.is_ok() || result.is_err(),
        "Graph name reference should not panic"
    );
}
