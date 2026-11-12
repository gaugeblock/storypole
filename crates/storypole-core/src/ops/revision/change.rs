//! Capturing the change a receipt certifies (`SP-41`).
//!
//! Content rather than position: a window of SHAs can be orphaned by a rebase, a squash or a
//! force-push, and the suite already had 44 edges whose target no longer resolved. Split out of
//! `revision.rs` because that file reached 243 lines and the capture is the part that grows.

use std::path::Path;

/// The change a receipt certifies, captured as content rather than as a position in history (`SP-41`).
///
/// A window of SHAs is a pointer, and a rebase, squash or force-push makes it dangle - the suite already
/// had 44 edges whose target no longer resolved. Content survives the rewrite, so the receipt records the
/// diff's hash (and the diff itself, capped) beside the window rather than instead of it.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CapturedChange {
    /// The diff's git blob id, which is a hash of the content and nothing else.
    pub hash: Option<String>,
    /// The diff text, truncated at [`MAX_DIFF_BYTES`] with the truncation marked.
    pub diff: Option<String>,
    /// Whether both ends of the window still exist. `None` when git could not answer.
    pub window_intact: Option<bool>,
}

/// How much of a diff a receipt carries. Big enough for a real change, small enough that replaying a
/// ledger does not pay for one large one.
pub const MAX_DIFF_BYTES: usize = 32 * 1024;

/// Captures the change between `base` and `head`, for the receipt (`SP-41`).
///
/// The run is deliberately unconditional rather than "only when the window looks suspect": orphaning is
/// discovered *later*, by whoever reads the receipt, and a record that only exists when the author
/// expected trouble is not a record.
#[must_use]
pub fn capture_change(dir: &Path, base: Option<&str>, head: Option<&str>) -> CapturedChange {
    let Some(head) = head else {
        return CapturedChange::default();
    };
    // `git diff <base> <head>` for a window, `git show <head>` when no base could be named - the same
    // fallback the window itself takes, so the diff and the window never describe different things.
    let text = match base {
        Some(base) => git_capture(dir, &["diff", base, head]),
        None => git_capture(dir, &["show", "--format=", head]),
    };
    let Some(text) = text.filter(|text| !text.trim().is_empty()) else {
        return CapturedChange {
            window_intact: window_exists(dir, base, Some(head)),
            ..CapturedChange::default()
        };
    };

    CapturedChange {
        // `git hash-object` hashes the content and nothing else, and it is git's own id space - so any
        // reader can ask git what it thinks of the same text, and no new dependency enters the crate.
        hash: git_capture_with_stdin(dir, &["hash-object", "--stdin"], &text),
        diff: Some(cap(&text)),
        window_intact: window_exists(dir, base, Some(head)),
    }
}

/// Whether every sha in the window still exists as a commit.
///
/// `Some(false)` is the orphaned case: the receipt is about to become the only record of what it
/// certified, which is worth saying out loud rather than discovering at the audit.
#[must_use]
pub fn window_exists(dir: &Path, base: Option<&str>, head: Option<&str>) -> Option<bool> {
    // A directory that is not a repository cannot answer, and "cannot answer" must not read as "the
    // commit is gone" (`SP-41`): the first live run of the reader warning claimed a receipt's window had
    // been rewritten when the only problem was that it pointed one directory too deep.
    gaugeblock_git::Repository::discover(dir).ok()?;
    let mut shas: Vec<&str> = Vec::new();
    if let Some(base) = base {
        shas.push(base);
    }
    if let Some(head) = head {
        shas.push(head);
    }
    if shas.is_empty() {
        return None;
    }
    let mut known = true;
    for sha in shas {
        match git_capture(dir, &["cat-file", "-e", &format!("{sha}^{{commit}}")]) {
            // `cat-file -e` prints nothing on success, so a Some(empty) means yes and None means no.
            Some(_) => known = known && true,
            None => known = false,
        }
    }
    Some(known)
}

/// Runs git and returns trimmed stdout, or `None` when it ran and produced nothing.
fn git_capture(dir: &Path, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Runs git with `stdin` fed in, for the one command that hashes text.
fn git_capture_with_stdin(dir: &Path, args: &[&str], stdin: &str) -> Option<String> {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .ok()?;
    child.stdin.as_mut()?.write_all(stdin.as_bytes()).ok()?;
    let output = child.wait_with_output().ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!value.is_empty()).then_some(value)
}

/// Truncates a diff to [`MAX_DIFF_BYTES`], marking the cut so a short diff is never mistaken for a whole
/// one.
fn cap(text: &str) -> String {
    if text.len() <= MAX_DIFF_BYTES {
        return text.to_string();
    }
    let mut cut = MAX_DIFF_BYTES;
    while cut > 0 && !text.is_char_boundary(cut) {
        cut -= 1;
    }
    format!(
        "{}\n[diff truncated at {MAX_DIFF_BYTES} bytes of {}; diff_hash identifies the full change]\n",
        &text[..cut],
        text.len()
    )
}
