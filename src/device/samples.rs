//! SDS sample upload over a [`MidiLink`].
//!
//! Handshaking sender: after the header and each packet, wait for the
//! receiver's ACK/NAK/WAIT. A silent receiver downgrades the transfer to
//! open-loop mode (fixed inter-packet rest), which the SDS spec allows.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::audio::ConvertedSample;
use crate::config::{SDS_HANDSHAKE_TIMEOUT, SDS_OPEN_LOOP_REST, SDS_RETRY_LIMIT};
use crate::midi::link::MidiLink;
use crate::midi::sds::{self, Handshake};

/// Progress of a running upload.
#[derive(Debug, Clone, Default)]
pub struct UploadProgress {
    pub packets_sent: usize,
    pub packets_total: usize,
}

/// Outcome of an upload attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UploadOutcome {
    /// All packets sent (and, in handshake mode, acknowledged).
    Sent {
        open_loop: bool,
    },
    Cancelled,
    /// The receiver cancelled or NAK-ed past the retry limit.
    Rejected(String),
    /// The link failed mid-transfer.
    LinkFailed(String),
}

/// Uploads one converted sample as SDS sample `sample_number`.
///
/// The Rytm stores it in the +Drive directory marked UPLOAD HERE
/// (`INCOMING/` by default) and names it via the SDS name extension.
pub fn upload_sample(
    link: &mut MidiLink,
    channel: u8,
    sample_number: u16,
    sample: &ConvertedSample,
    cancel: &Arc<AtomicBool>,
    mut progress: impl FnMut(UploadProgress),
) -> UploadOutcome {
    let packets = sds::data_packets(channel, &sample.pcm);
    let total = packets.len();

    let header = sds::dump_header(
        channel,
        sample_number,
        crate::audio::NATIVE_SAMPLE_RATE,
        sample.pcm.len() as u32,
    );
    if let Err(e) = link.send(&header) {
        return UploadOutcome::LinkFailed(e.to_string());
    }
    // A receiver that says nothing to the header gets the dump open-loop.
    let open_loop = match await_handshake(link, SDS_HANDSHAKE_TIMEOUT) {
        Some(Handshake::Cancel(_)) => return UploadOutcome::Rejected("cancelled by device".into()),
        Some(_) => false,
        None => true,
    };

    for (i, packet) in packets.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return UploadOutcome::Cancelled;
        }
        let mut retries = 0;
        loop {
            if let Err(e) = link.send(packet) {
                return UploadOutcome::LinkFailed(e.to_string());
            }
            if open_loop {
                std::thread::sleep(SDS_OPEN_LOOP_REST);
                break;
            }
            match await_handshake(link, SDS_HANDSHAKE_TIMEOUT) {
                Some(Handshake::Ack(_)) | None => break,
                Some(Handshake::Wait(_)) => {
                    // Busy: block until the next definitive reply.
                    match await_handshake(link, SDS_HANDSHAKE_TIMEOUT * 10) {
                        Some(Handshake::Cancel(_)) => {
                            return UploadOutcome::Rejected("cancelled by device".into())
                        }
                        Some(Handshake::Nak(_)) => {}
                        _ => break,
                    }
                }
                Some(Handshake::Cancel(_)) => {
                    return UploadOutcome::Rejected("cancelled by device".into())
                }
                Some(Handshake::Nak(_)) => {}
            }
            retries += 1;
            if retries > SDS_RETRY_LIMIT {
                return UploadOutcome::Rejected(format!("packet {i} refused after retries"));
            }
        }
        progress(UploadProgress {
            packets_sent: i + 1,
            packets_total: total,
        });
    }

    // Name the sample (extension message; harmless if ignored).
    let _ = link.send(&sds::sample_name(channel, sample_number, &sample.name));

    UploadOutcome::Sent { open_loop }
}

fn await_handshake(link: &MidiLink, timeout: Duration) -> Option<Handshake> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        let remaining = deadline.checked_duration_since(std::time::Instant::now())?;
        let msg = link.recv_timeout(remaining)?;
        if let Some(handshake) = sds::parse_handshake(&msg) {
            return Some(handshake);
        }
    }
}
