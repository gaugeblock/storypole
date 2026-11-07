//! Superseding a decision and the chain it drags with it.
//!
//! Split from replay when the file passed the 250-line limit (`GB-291`). Replay applies events in order;
//! a cascade is the one place where applying an event reaches back into decisions already recorded.

use super::*;
pub(crate) fn cascade_supersede(
    task: &mut TaskNode,
    decision_id: Uuid,
    reason: &str,
    timestamp_ms: u64,
) {
    let mut to_supersede = vec![decision_id];
    let mut idx = 0;
    while idx < to_supersede.len() {
        let current = to_supersede[idx];
        idx += 1;
        for d in &task.decisions {
            if d.parent_decision_id == Some(current) && !to_supersede.contains(&d.decision_id) {
                to_supersede.push(d.decision_id);
            }
        }
    }

    for d in &mut task.decisions {
        if to_supersede.contains(&d.decision_id) {
            d.status = DecisionStatus::Superseded;
            d.updated_at_epoch_ms = timestamp_ms;
            let note = format!(" [Superseded: {reason}]");
            if let Some(ref mut r) = d.rationale {
                r.push_str(&note);
            } else {
                d.rationale = Some(note);
            }
        }
    }
}
