//! What counts as a citation, and how one is replaced (`GB-347`, `GB-351`).
//!
//! The rules live apart from the document shape they are applied to, because two passes share the shape and disagree
//! about the rules: a remint moves keys *and* the files named for them, so it rewrites a path; a relink moves nothing, so
//! there a path is data and only a bare key is a reference. Naming that difference once stopped it being re-derived by
//! hand at each call site.

use serde_json::Value;
use storypole_core::ticket_keys::Remint;

/// Whether `haystack` cites `key` somewhere other than inside a path.
///
/// `plans/GB-64.md` names a file; `GB-64` alone names a ticket. A remint moves keys *and* the files named for them, so it
/// rewrites both. A pass that moves nothing must not treat a filename as a reference - doing so produced three links
/// from a ticket that merely quoted the filenames it was describing (`GB-351`).
pub(crate) fn cites_bare(haystack: &str, key: &str) -> bool {
    haystack.match_indices(key).any(|(at, _)| {
        let before = haystack[..at].chars().next_back();
        let after = haystack[at + key.len()..].chars().next();
        let boundary = |c: Option<char>| c.is_none_or(|c| !c.is_alphanumeric() && c != '-');
        boundary(before) && boundary(after) && !inside_path(haystack, at, key.len())
    })
}

/// Whether the match at `at` sits in a path: behind a separator, or in front of a file extension.
///
/// A trailing full stop is a sentence, not an extension, so the character after the dot decides.
fn inside_path(text: &str, at: usize, len: usize) -> bool {
    if text[..at].ends_with('/') {
        return true;
    }
    let Some(tail) = text[at + len..].strip_prefix('.') else {
        return false;
    };
    let extension: String = tail
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .collect();
    extension.len() >= 2
}

/// Replaces whole-token occurrences of `from` with `to`, leaving `GB-1` inside `GB-10` alone.
///
/// A hyphen is not a boundary, deliberately: it keeps the rewrite out of branch names (`feat/GB-123-x`) and slugs, which
/// record where work happened rather than which ticket it belongs to. Under [`Citations::Bare`] a match inside a path is
/// left alone as well, because there it names a file rather than a ticket (`GB-351`).
pub(crate) fn replace_token(text: &str, from: &str, to: &str, citations: Citations) -> String {
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    while let Some(offset) = text[cursor..].find(from) {
        let at = cursor + offset;
        let before = text[..at].chars().next_back();
        let after = text[at + from.len()..].chars().next();
        let boundary = |c: Option<char>| c.is_none_or(|c| !c.is_alphanumeric() && c != '-');
        let cites = boundary(before)
            && boundary(after)
            && (citations == Citations::Anywhere || !inside_path(text, at, from.len()));
        out.push_str(&text[cursor..at]);
        out.push_str(if cites { to } else { from });
        cursor = at + from.len();
    }
    out.push_str(&text[cursor..]);
    out
}

/// Whether `haystack` cites `key` as a whole token, so `GB-1` does not match inside `GB-10`.
pub(crate) fn cites(haystack: &str, key: &str) -> bool {
    haystack.match_indices(key).any(|(at, _)| {
        let before = haystack[..at].chars().next_back();
        let after = haystack[at + key.len()..].chars().next();
        let boundary = |c: Option<char>| c.is_none_or(|c| !c.is_alphanumeric() && c != '-');
        boundary(before) && boundary(after)
    })
}

/// Rewrites the citations in one prose field, returning how many keys moved.
pub(crate) fn rewrite_prose(
    container: &mut Value,
    field: &str,
    mapping: &[Remint],
    rules: Rewrite,
) -> usize {
    let Some(object) = container.as_object_mut() else {
        return 0;
    };
    let Some(current) = object
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_string)
    else {
        return 0;
    };
    let (body, moved) = rewrite_text(&current, mapping, rules);
    if moved > 0 {
        object.insert(field.to_string(), Value::String(body));
    }
    moved
}

/// Rewrites the citations in every string of an array field, returning how many keys moved.
pub(crate) fn rewrite_prose_list(
    container: &mut Value,
    field: &str,
    mapping: &[Remint],
    rules: Rewrite,
) -> usize {
    let Some(object) = container.as_object_mut() else {
        return 0;
    };
    let Some(items) = object.get_mut(field).and_then(Value::as_array_mut) else {
        return 0;
    };
    let mut moved = 0;
    for item in items {
        let Some(current) = item.as_str().map(str::to_string) else {
            continue;
        };
        let (body, count) = rewrite_text(&current, mapping, rules);
        if count > 0 {
            *item = Value::String(body);
        }
        moved += count;
    }
    moved
}

/// Applies every mapping entry to one string, counting the keys that were actually cited.
fn rewrite_text(text: &str, mapping: &[Remint], rules: Rewrite) -> (String, usize) {
    let mut body = text.to_string();
    let mut moved = 0;
    for remint in mapping {
        let cited = match rules.citations {
            Citations::Anywhere => cites(&body, &remint.from),
            Citations::Bare => cites_bare(&body, &remint.from),
        };
        if cited {
            body = replace_token(&body, &remint.from, &remint.to, rules.citations);
            moved += 1;
        }
    }
    (body, moved)
}

/// Which occurrences of a key count as a citation (`GB-351`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Citations {
    /// Every whole-token occurrence, including one inside a path. A remint moves keys *and* the files named for them, so
    /// a description saying `plans/GB-64.md` has to follow the rename.
    Anywhere,
    /// Only occurrences outside a path. `plans/GB-64.md` names a file, `GB-64` names a ticket, and a pass that moves
    /// nothing must not treat the first as a reference.
    Bare,
}

/// Whether an attached plan's stored path moves with the key.
///
/// A `remint` renames the blueprint beside the ticket, so the path moves with it. A `relink` repairs references and
/// moves nothing, so rewriting the path there would point at a file that was never renamed (`GB-351`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlanPath {
    /// The blueprint is renamed in the same pass; update the pointer.
    Moves,
    /// Nothing moves on disk; leave the pointer as written.
    Stays,
}

/// Everything one pass needs to know about what a reference is.
///
/// Two passes rewrite citations and disagree about exactly two things, so the disagreement is written down once here and
/// named at the call site rather than threaded as separate flags that could be paired wrongly.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Rewrite {
    /// Which occurrences count as a citation.
    pub(crate) citations: Citations,
    /// Whether a plan's stored path moves with the key.
    pub(crate) plan_path: PlanPath,
}

impl Rewrite {
    /// A remint: keys move, and so do the files named for them.
    pub(crate) const REMINT: Self = Self {
        citations: Citations::Anywhere,
        plan_path: PlanPath::Moves,
    };
    /// A relink: nothing moves, so a path is data and a filename is not evidence.
    pub(crate) const RELINK: Self = Self {
        citations: Citations::Bare,
        plan_path: PlanPath::Stays,
    };
}
