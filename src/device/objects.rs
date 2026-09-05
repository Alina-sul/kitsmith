//! Object sync engine: pulls the active project's kits/patterns/sounds over
//! the AR object-sysex protocol (rytm-rs) through a [`MidiLink`].
//!
//! Query → wait for the reply → feed it to `RytmProject`. The device answers
//! on its own schedule (sysex runs on a low-priority thread in its RTOS), so
//! every reply is awaited with a timeout and unrelated sysex is skipped.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use rytm_rs::prelude::*;
use rytm_rs::RytmProject;

use crate::config::{OBJECT_REPLY_TIMEOUT, SYNC_MISS_LIMIT};
use crate::midi::link::MidiLink;
use crate::model::ProjectModel;

/// Progress of a running sync, published after every object.
#[derive(Debug, Clone, Default)]
pub struct SyncProgress {
    pub stage: &'static str,
    pub done: usize,
    pub total: usize,
    /// Objects that did not answer within the timeout.
    pub missed: usize,
}

/// One object the sync could not read, and how it failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissedObject {
    /// e.g. `kit 07`, `pattern B03`, `pool sound 041`.
    pub what: String,
    /// True when sysex arrived but rytm-rs did not accept it (parse/version
    /// problem); false when the device stayed silent (timeout).
    pub got_reply: bool,
}

impl MissedObject {
    pub fn describe(&self) -> String {
        if self.got_reply {
            format!("{} — reply received but not understood", self.what)
        } else {
            format!("{} — no reply within the timeout", self.what)
        }
    }
}

/// Outcome of a sync run.
#[derive(Debug)]
pub enum SyncOutcome {
    /// Model built; `missed` lists unread objects (empty = complete pull).
    Complete {
        model: ProjectModel,
        missed: Vec<MissedObject>,
    },
    /// Cancelled from the UI; nothing published.
    Cancelled,
    /// Too many consecutive silent objects — device gone or not listening.
    DeviceSilent,
}

/// Pulls kits, patterns, and pool sounds of the active project.
///
/// `cancel` is checked between objects; `progress` fires after each one.
pub fn sync_active_project(
    link: &mut MidiLink,
    cancel: &Arc<AtomicBool>,
    mut progress: impl FnMut(SyncProgress),
) -> Result<SyncOutcome, rytm_rs::error::RytmError> {
    let mut rytm = RytmProject::try_default()?;
    let mut missed: Vec<MissedObject> = Vec::new();
    let mut consecutive_misses = 0usize;

    let kit_queries: Vec<(&'static str, String, Vec<u8>)> = (0..128)
        .map(|i| {
            Ok((
                "kits",
                format!("kit {:02}", i + 1),
                KitQuery::new(i)?.as_sysex()?,
            ))
        })
        .collect::<Result<_, rytm_rs::error::RytmError>>()?;
    let pattern_queries: Vec<(&'static str, String, Vec<u8>)> = (0..128)
        .map(|i| {
            Ok((
                "patterns",
                format!("pattern {}", crate::model::pattern_display_name(i)),
                PatternQuery::new(i)?.as_sysex()?,
            ))
        })
        .collect::<Result<_, rytm_rs::error::RytmError>>()?;
    let sound_queries: Vec<(&'static str, String, Vec<u8>)> = (0..128)
        .map(|i| {
            Ok((
                "pool sounds",
                format!("pool sound {:03}", i + 1),
                SoundQuery::new(i)?.as_sysex()?,
            ))
        })
        .collect::<Result<_, rytm_rs::error::RytmError>>()?;

    let queries: Vec<(&'static str, String, Vec<u8>)> = kit_queries
        .into_iter()
        .chain(pattern_queries)
        .chain(sound_queries)
        .collect();
    let total = queries.len();

    for (done, (stage, what, query)) in queries.into_iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Ok(SyncOutcome::Cancelled);
        }
        if link.send(&query).is_err() {
            return Ok(SyncOutcome::DeviceSilent);
        }
        match await_reply(link, &mut rytm) {
            ReplyResult::Accepted => consecutive_misses = 0,
            outcome => {
                missed.push(MissedObject {
                    what,
                    got_reply: outcome == ReplyResult::ReceivedButRejected,
                });
                consecutive_misses += 1;
                if consecutive_misses >= SYNC_MISS_LIMIT {
                    return Ok(SyncOutcome::DeviceSilent);
                }
            }
        }
        progress(SyncProgress {
            stage,
            done: done + 1,
            total,
            missed: missed.len(),
        });
    }

    Ok(SyncOutcome::Complete {
        model: ProjectModel::from_rytm(&rytm),
        missed,
    })
}

/// How waiting for one object's reply ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReplyResult {
    /// A reply was accepted into the project.
    Accepted,
    /// Sysex arrived within the timeout but none of it was accepted —
    /// the device answered, rytm-rs could not parse it.
    ReceivedButRejected,
    /// Nothing arrived at all.
    Silent,
}

/// Waits for the reply to the last query, feeding every received sysex to the
/// project, and reports how the wait ended.
fn await_reply(link: &MidiLink, rytm: &mut RytmProject) -> ReplyResult {
    let deadline = std::time::Instant::now() + OBJECT_REPLY_TIMEOUT;
    let mut received_any = false;
    loop {
        let Some(remaining) = deadline.checked_duration_since(std::time::Instant::now()) else {
            return if received_any {
                ReplyResult::ReceivedButRejected
            } else {
                ReplyResult::Silent
            };
        };
        let Some(msg) = link.recv_timeout(remaining) else {
            return if received_any {
                ReplyResult::ReceivedButRejected
            } else {
                ReplyResult::Silent
            };
        };
        received_any = true;
        // rytm-rs can panic on objects it does not understand — treat that
        // as a rejected reply, not an app crash.
        let accepted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            rytm.update_from_sysex_response(&msg).is_ok()
        }))
        .unwrap_or(false);
        if accepted {
            return ReplyResult::Accepted;
        }
    }
}

/// Waits for one reply the project accepts and returns its raw bytes
/// (`None` on timeout). Non-AR sysex is skipped, not fatal.
pub fn accept_reply(link: &MidiLink, rytm: &mut RytmProject) -> Option<Vec<u8>> {
    let deadline = std::time::Instant::now() + OBJECT_REPLY_TIMEOUT;
    loop {
        let remaining = deadline.checked_duration_since(std::time::Instant::now())?;
        let msg = link.recv_timeout(remaining)?;
        if rytm.update_from_sysex_response(&msg).is_ok() {
            return Some(msg);
        }
    }
}
