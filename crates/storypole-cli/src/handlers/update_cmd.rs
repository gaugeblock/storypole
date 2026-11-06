//! Handler for `storypole update`.

use crate::update_args::UpdateArgs;
use anyhow::Result;
use storypole_core::StorypoleStore;
use storypole_core::index::query::{get_task_by_id, resolve_task_id};
use storypole_core::model::task::TaskScope;
use storypole_core::ops::mutations::{UpdateTaskParams, update_task};

/// Execute task update.
pub fn handle_update(store: &mut StorypoleStore, args: UpdateArgs, json: bool) -> Result<()> {
    let id = resolve_task_id(store.conn(), &args.identifier)?;

    // `--criterion-file` is appended to any inline criteria, and read before anything mutates so a bad
    // path fails the command rather than half-applying it (`GB-250`).
    let mut criteria = args.criteria.clone();
    if let Some(source) = &args.criterion_file {
        let text = if source == "-" {
            let mut buffer = String::new();
            std::io::Read::read_to_string(&mut std::io::stdin(), &mut buffer)?;
            buffer
        } else {
            std::fs::read_to_string(source).map_err(|error| {
                anyhow::anyhow!("cannot read criterion file `{source}`: {error}")
            })?
        };
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            criteria.push(trimmed.to_string());
        }
    }
    let has_scope = args.repo.is_some()
        || args.crate_name.is_some()
        || args.module.is_some()
        || !args.files.is_empty()
        || !args.symbols.is_empty()
        || !args.labels.is_empty();

    let scope = if has_scope {
        let existing = get_task_by_id(store.conn(), id)?;
        Some(TaskScope {
            repo: args.repo.or(existing.scope.repo),
            crate_name: args.crate_name.or(existing.scope.crate_name),
            module_path: args.module.or(existing.scope.module_path),
            target_files: if !args.files.is_empty() {
                args.files
            } else {
                existing.scope.target_files
            },
            target_symbols: if !args.symbols.is_empty() {
                args.symbols
            } else {
                existing.scope.target_symbols
            },
            labels: if !args.labels.is_empty() {
                args.labels
            } else {
                existing.scope.labels
            },
        })
    } else {
        None
    };

    let params = UpdateTaskParams {
        title: args.title,
        description: args.desc,
        priority: args.priority,
        scope,
        acceptance_criteria: if !criteria.is_empty() {
            Some(criteria)
        } else {
            None
        },
    };

    update_task(store, id, params)?;
    let updated = get_task_by_id(store.conn(), id)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&updated)?);
    } else {
        println!(
            "Updated task {} [{:?}] P{}: {}",
            updated.ticket_key, updated.kind, updated.priority, updated.title
        );
    }
    Ok(())
}
