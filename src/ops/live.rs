//! The real [`DeviceBackend`]: object sysex over a [`MidiLink`] via rytm-rs.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use rytm_rs::prelude::*;
use rytm_rs::RytmProject;

use super::{DeviceBackend, OpError};
use crate::cache;
use crate::device::objects::accept_reply;
use crate::midi::link::MidiLink;
use crate::model::slot_from_wire;

/// Backend talking to a connected Rytm.
pub struct LiveBackend {
    link: MidiLink,
    rytm: RytmProject,
}

impl LiveBackend {
    /// Wraps an open link.
    pub fn new(link: MidiLink) -> Result<Self, OpError> {
        Ok(Self {
            link,
            rytm: RytmProject::try_default().map_err(|e| OpError::Device(e.to_string()))?,
        })
    }

    /// Queries one kit into the internal project; returns the raw reply.
    fn query_kit(&mut self, kit_index: usize) -> Result<Vec<u8>, OpError> {
        let query = KitQuery::new(kit_index)
            .and_then(|q| q.as_sysex())
            .map_err(|e| OpError::Device(e.to_string()))?;
        self.exchange(&query)
            .ok_or_else(|| OpError::Device(format!("kit {kit_index} query got no reply")))
    }

    /// Sends `msg` and waits for an accepted reply.
    fn exchange(&mut self, msg: &[u8]) -> Option<Vec<u8>> {
        self.link.send(msg).ok()?;
        accept_reply(&self.link, &mut self.rytm)
    }
}

impl DeviceBackend for LiveBackend {
    fn backup_project(&mut self) -> Result<PathBuf, OpError> {
        let mut queries: Vec<Vec<u8>> = Vec::new();
        let err = |e: rytm_rs::error::RytmError| OpError::Device(e.to_string());
        for i in 0..128 {
            queries.push(KitQuery::new(i).and_then(|q| q.as_sysex()).map_err(err)?);
            queries.push(
                PatternQuery::new(i)
                    .and_then(|q| q.as_sysex())
                    .map_err(err)?,
            );
            queries.push(SoundQuery::new(i).and_then(|q| q.as_sysex()).map_err(err)?);
        }
        for i in 0..4 {
            queries.push(
                GlobalQuery::new(i)
                    .and_then(|q| q.as_sysex())
                    .map_err(err)?,
            );
        }
        queries.push(SettingsQuery::new().as_sysex().map_err(err)?);

        let mut raw = Vec::new();
        let mut replies = 0usize;
        for query in &queries {
            if let Some(reply) = self.exchange(query) {
                raw.extend_from_slice(&reply);
                replies += 1;
            }
        }
        if replies == 0 {
            return Err(OpError::Device("backup got no replies at all".into()));
        }

        let dir = cache::default_dir().join("backups");
        std::fs::create_dir_all(&dir).map_err(|e| OpError::Device(e.to_string()))?;
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let path = dir.join(format!("backup-{stamp}-{replies}-objects.syx"));
        std::fs::write(&path, &raw).map_err(|e| OpError::Device(e.to_string()))?;
        Ok(path)
    }

    fn write_kit_sample_slot(
        &mut self,
        kit_index: usize,
        track: usize,
        slot: u8,
    ) -> Result<(), OpError> {
        // Always mutate a fresh copy of the kit, never a stale cache.
        self.query_kit(kit_index)?;
        let kit = self
            .rytm
            .kits_mut()
            .get_mut(kit_index)
            .ok_or_else(|| OpError::InvalidRequest(format!("no kit {kit_index}")))?;
        let sound = kit
            .sounds_mut()
            .get_mut(track)
            .ok_or_else(|| OpError::InvalidRequest(format!("no track {track}")))?;
        sound
            .sample_mut()
            .set_slice_number(usize::from(slot))
            .map_err(|e| OpError::InvalidRequest(e.to_string()))?;
        let msg = kit.as_sysex().map_err(|e| OpError::Device(e.to_string()))?;
        self.link
            .send(&msg)
            .map_err(|e| OpError::Device(e.to_string()))
    }

    fn read_kit_sample_slot(
        &mut self,
        kit_index: usize,
        track: usize,
    ) -> Result<Option<u8>, OpError> {
        self.query_kit(kit_index)?;
        let sound = self
            .rytm
            .kits()
            .get(kit_index)
            .and_then(|kit| kit.sounds().get(track))
            .ok_or_else(|| OpError::InvalidRequest(format!("no kit {kit_index} track {track}")))?;
        Ok(slot_from_wire(sound.sample().slice_number()))
    }
}
