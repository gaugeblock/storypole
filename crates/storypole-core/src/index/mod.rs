//! SQLite relational index for fast querying, filtering, and dual ID resolution.

pub mod query;

mod insert_task_record;
mod rebuild;
mod schema;

pub use insert_task_record::insert_task_record;
pub use rebuild::{open_and_sync_index, rebuild_index};
pub use schema::{apply_connection_pragmas, init_schema};
