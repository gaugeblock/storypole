//! Handler for `storypole update`.

use crate::json_out::print_json;
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
    // `--append-desc` exists so a progress note does not require re-sending the whole description (`GB-250`).
    // The appended text is resolved before anything mutates, for the same reason as the criterion file: a bad
    // path must fail the command rather than half-apply it.
    let appended = match (&args.append_desc, &args.append_desc_file) {
        (Some(inline), None) => Some(inline.clone()),
        (None, Some(source)) => Some(read_source(source)?),
        (None, None) => None,
        (Some(_), Some(_)) => {
            anyhow::bail!("--append-desc and --append-desc-file are the same thing said twice")
        }
    };
    let description = match appended {
        Some(addition) => {
            let existing = get_task_by_id(store.conn(), id)?.description;
            Some(append_paragraph(&existing, &addition))
        }
        None => args.desc,
    };

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
        description,
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
        print_json("update", &serde_json::json!({"task": updated}))?;
    } else {
        println!(
            "Updated task {} [{:?}] P{}: {}",
            updated.ticket_key, updated.kind, updated.priority, updated.title
        );
    }
    Ok(())
}

/// Reads a file, or stdin when the source is `-`.
fn read_source(source: &str) -> Result<String> {
    if source == "-" {
        let mut buffer = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut buffer)?;
        Ok(buffer)
    } else {
        std::fs::read_to_string(source)
            .map_err(|error| anyhow::anyhow!("cannot read `{source}`: {error}"))
    }
}

/// Joins an existing description and an addition with exactly one blank line.
///
/// Idempotent in the way that matters: appending to an empty description yields the addition alone, with no
/// leading newlines, so a task that never had a description does not gain a blank first line. Trailing
/// whitespace on either side is dropped rather than accumulated, because every append would otherwise add
/// another separator to the end of the field.
fn append_paragraph(existing: &str, addition: &str) -> String {
    let addition = addition.trim();
    let existing = existing.trim_end();
    if existing.is_empty() {
        addition.to_string()
    } else if addition.is_empty() {
        existing.to_string()
    } else {
        format!(
            "{existing}

{addition}"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::append_paragraph;

    #[test]
    fn an_empty_description_takes_the_addition_without_a_leading_blank_line() {
        assert_eq!(append_paragraph("", "first note"), "first note");
        assert_eq!(append_paragraph("   ", "first note"), "first note");
    }

    #[test]
    fn a_second_note_is_separated_by_one_blank_line() {
        assert_eq!(
            append_paragraph("first note", "second note"),
            "first note

second note"
        );
    }

    #[test]
    fn trailing_whitespace_is_not_accumulated_across_appends() {
        // Every append would otherwise add another separator to the end of the field.
        let once = append_paragraph(
            "first note

",
            "second note
",
        );
        let twice = append_paragraph(&once, "third note");
        assert_eq!(
            once,
            "first note

second note"
        );
        assert_eq!(
            twice,
            "first note

second note

third note"
        );
    }

    #[test]
    fn an_empty_addition_changes_nothing() {
        assert_eq!(
            append_paragraph(
                "first note",
                "  
 "
            ),
            "first note"
        );
    }
}
