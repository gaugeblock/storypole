//! Cross-repository edge locators in the index (`GB-332`).
//!
//! An edge's identity is the target's uuid. That is enough to traverse a DAG inside one store and useless in a
//! checkout that does not hold the target - which, since ADR 0009 put tickets in their own repositories, is most
//! of them. The locator columns carry the ticket key and repository so a reader can render `GB-316 (gaugeblock)`
//! instead of a uuid nobody can open.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use gaugeblock_contracts::storypole::{EdgeKind, TaskKind, TaskScope, TaskStatus};
use gaugeblock_contracts::{TaskEdge, TaskNode};
use rusqlite::Connection;
use storypole_core::index::{apply_connection_pragmas, init_schema, insert_task_record};
use uuid::Uuid;

fn task_with_edge(target_ticket_key: Option<&str>, target_repo: Option<&str>) -> TaskNode {
    TaskNode {
        id: Uuid::from_u128(1),
        ticket_key: "GB-1".to_string(),
        slug: "gb-1".to_string(),
        title: "a ticket".to_string(),
        description: String::new(),
        kind: TaskKind::Feature,
        status: TaskStatus::Ready,
        priority: 3,
        scope: TaskScope::default(),
        edges: vec![TaskEdge {
            kind: EdgeKind::ParentEpic,
            target_id: Uuid::from_u128(99),
            target_ticket_key: target_ticket_key.map(str::to_string),
            target_repo: target_repo.map(str::to_string),
        }],
        decisions: Vec::new(),
        acceptance_criteria: Vec::new(),
        plan: None,
        active_claim: None,
        latest_handoff: None,
        verification_receipt: None,
        created_at_epoch_ms: 1,
        updated_at_epoch_ms: 1,
        archived_at_epoch_ms: None,
    }
}

fn indexed(conn: &Connection) -> (Option<String>, Option<String>) {
    conn.query_row(
        "SELECT target_ticket_key, target_repo FROM task_edges",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .expect("one edge")
}

#[test]
fn an_edge_keeps_the_locator_that_lets_another_checkout_render_it() {
    let conn = Connection::open_in_memory().unwrap();
    apply_connection_pragmas(&conn).unwrap();
    init_schema(&conn).unwrap();

    insert_task_record(&conn, &task_with_edge(Some("GB-316"), Some("gaugeblock"))).unwrap();

    assert_eq!(
        indexed(&conn),
        (Some("GB-316".to_string()), Some("gaugeblock".to_string()))
    );
}

#[test]
fn an_edge_without_a_locator_is_still_an_edge() {
    // Most edges inside one repository never recorded one; that is a gap in rendering, not a broken edge.
    let conn = Connection::open_in_memory().unwrap();
    init_schema(&conn).unwrap();

    insert_task_record(&conn, &task_with_edge(None, None)).unwrap();

    assert_eq!(indexed(&conn), (None, None));
}

#[test]
fn an_index_built_before_the_columns_existed_is_upgraded() {
    // `CREATE TABLE IF NOT EXISTS` cannot alter a table, so an older index would keep the three-column shape and
    // silently drop the locator on every rebuild. This is the case the additive upgrade exists for.
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE task_edges (
            task_id TEXT NOT NULL,
            kind TEXT NOT NULL,
            target_id TEXT NOT NULL,
            PRIMARY KEY (task_id, kind, target_id)
        );",
    )
    .unwrap();

    init_schema(&conn).unwrap();

    insert_task_record(&conn, &task_with_edge(Some("GB-316"), Some("gaugeblock"))).unwrap();
    assert_eq!(
        indexed(&conn),
        (Some("GB-316".to_string()), Some("gaugeblock".to_string()))
    );
}
