use std::path::Path;

use super::{ResolvedScope, ScopeSource, resolve_scope};

fn members() -> Vec<String> {
    vec![
        "miter".to_string(),
        "storypole".to_string(),
        "common".to_string(),
        "benchwork".to_string(),
    ]
}

#[test]
fn an_explicit_argument_wins_over_everything() {
    let resolved = resolve_scope(
        Some("benchwork"),
        Some("storypole"),
        &["miter/src/lib.rs".to_string()],
        &[],
        &members(),
        None,
        Path::new("/suite"),
    );
    assert_eq!(resolved.repo.as_deref(), Some("benchwork"));
    assert_eq!(resolved.source, ScopeSource::Explicit);
}

#[test]
fn a_persisted_scope_is_not_second_guessed() {
    // The whole point of persisting at claim: later commands read the decision, they do not remake it.
    let resolved = resolve_scope(
        None,
        Some("storypole"),
        &["miter/src/lib.rs".to_string()],
        &[],
        &members(),
        None,
        Path::new("/suite"),
    );
    assert_eq!(resolved.repo.as_deref(), Some("storypole"));
    assert_eq!(resolved.source, ScopeSource::Persisted);
}

#[test]
fn the_paths_a_ticket_names_place_it() {
    let resolved = resolve_scope(
        None,
        None,
        &["./miter/src/cli/ingest.rs".to_string()],
        &[],
        &members(),
        None,
        Path::new("/suite"),
    );
    assert_eq!(resolved.repo.as_deref(), Some("miter"));
    assert_eq!(resolved.source, ScopeSource::Files);
}

#[test]
fn a_path_that_merely_mentions_a_member_places_nothing() {
    // `docs/storypole-notes.md` is not storypole's work, and a substring match would say it was.
    let resolved = resolve_scope(
        None,
        None,
        &["docs/storypole-notes.md".to_string()],
        &[],
        &members(),
        None,
        Path::new("/suite"),
    );
    assert_eq!(resolved.repo, None);
    assert_eq!(resolved.source, ScopeSource::Harness);
}

#[test]
fn a_symbol_places_through_its_crate() {
    let resolved = resolve_scope(
        None,
        None,
        &[],
        &["storypole_core::store::StorypoleStore".to_string()],
        &members(),
        None,
        Path::new("/suite"),
    );
    assert_eq!(resolved.repo.as_deref(), Some("storypole"));
    assert_eq!(resolved.source, ScopeSource::Symbols);
}

#[test]
fn a_hyphenated_crate_places_by_its_first_segment() {
    let resolved = resolve_scope(
        None,
        None,
        &[],
        &["miter_cli::audit_paths".to_string()],
        &members(),
        None,
        Path::new("/suite"),
    );
    assert_eq!(resolved.repo.as_deref(), Some("miter"));
}

#[test]
fn the_working_directory_places_only_when_it_is_inside_a_member() {
    let inside = resolve_scope(
        None,
        None,
        &[],
        &[],
        &members(),
        Some(Path::new("/suite/miter/crates/engine")),
        Path::new("/suite"),
    );
    assert_eq!(inside.repo.as_deref(), Some("miter"));
    assert_eq!(inside.source, ScopeSource::WorkingDirectory);

    let outside = resolve_scope(
        None,
        None,
        &[],
        &[],
        &members(),
        Some(Path::new("/elsewhere/scratch")),
        Path::new("/suite"),
    );
    assert_eq!(outside.repo, None);
    assert_eq!(outside.source, ScopeSource::Harness);
}

#[test]
fn nothing_named_a_repository_and_the_answer_says_so() {
    // Not a fallback, a finding: harness-level work is stated, never inferred from failure to detect.
    let resolved = resolve_scope(None, None, &[], &[], &members(), None, Path::new("/suite"));
    assert_eq!(
        resolved,
        ResolvedScope {
            repo: None,
            source: ScopeSource::Harness
        }
    );
}
