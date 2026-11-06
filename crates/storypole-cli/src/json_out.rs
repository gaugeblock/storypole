//! JSON output for the CLI, through the suite envelope (`GB-274`).
//!
//! Every `--json` path goes through [`print_json`] rather than calling `serde_json::to_string_pretty`
//! directly, so `schema_version` and `command` are present without each handler having to remember. The
//! shape is defined once in `gaugeblock_contracts::json` and documented in `CONTRACTS.md`; a handler that
//! prints JSON in another shape is a contract violation, not a style choice.

use anyhow::Result;

/// Prints `payload` inside the suite envelope, pretty-printed.
///
/// The payload must be an object: the envelope merges it beside `schema_version` and `command`, so an
/// array has to be named by the caller (`{"tasks": [...], "count": n}`). That naming is the point - it is
/// what makes the output extensible without a breaking change.
///
/// # Errors
///
/// Returns the serialisation error rather than printing a partial envelope.
pub fn print_json<T: ::serde::Serialize>(command: &str, payload: &T) -> Result<()> {
    let envelope = gaugeblock_contracts::json::envelope(command, payload)?;
    println!("{}", serde_json::to_string_pretty(&envelope)?);
    Ok(())
}
