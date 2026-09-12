//! The one place [`KitEdit`]s meet rytm-rs: applying an edit to a `Kit` and
//! extracting a field's current value for read-back verification. Everything
//! else (GUI, ops) speaks only the pure `model::edit` vocabulary.

use rytm_rs::object::Kit;

use crate::model::edit::{FieldValue, KitEdit, KitField};

/// Applies one edit to an in-memory kit (a fresh device read, never a cache).
pub fn apply(edit: &KitEdit, kit: &mut Kit) -> Result<(), String> {
    edit.validate()?;
    let e = |err: rytm_rs::error::RytmError| err.to_string();
    let field = edit.field;
    match (&edit.value, field) {
        (FieldValue::Text(name), KitField::KitName) => kit.set_name(name).map_err(e),
        (FieldValue::Text(name), KitField::SoundName(t)) => {
            sound_mut(kit, t)?.set_name(name).map_err(e)
        }
        (FieldValue::Int(v), KitField::TrackLevel(t)) => {
            kit.set_track_level(t, *v as usize).map_err(e)
        }
        (FieldValue::Toggle(v), KitField::SampleLoop(t)) => {
            sound_mut(kit, t)?.sample_mut().set_loop_flag(*v);
            Ok(())
        }
        (FieldValue::Float(v), KitField::SampleStart(t)) => {
            sound_mut(kit, t)?.sample_mut().set_start(*v).map_err(e)
        }
        (FieldValue::Float(v), KitField::SampleEnd(t)) => {
            sound_mut(kit, t)?.sample_mut().set_end(*v).map_err(e)
        }
        (FieldValue::Int(v), field) => {
            let track = field
                .track()
                .ok_or_else(|| format!("{} is not a track field", field.label()))?;
            let sound = sound_mut(kit, track)?;
            let v = *v;
            let signed = v as isize;
            let unsigned = usize::try_from(v).unwrap_or(0);
            match field {
                KitField::SampleTune(_) => sound.sample_mut().set_tune(signed).map_err(e),
                KitField::SampleFineTune(_) => sound.sample_mut().set_fine_tune(signed).map_err(e),
                KitField::SampleBitReduction(_) => {
                    sound.sample_mut().set_bit_reduction(unsigned).map_err(e)
                }
                KitField::SampleLevel(_) => sound.sample_mut().set_volume(unsigned).map_err(e),
                KitField::AmpAttack(_) => sound.amplitude_mut().set_attack(unsigned).map_err(e),
                KitField::AmpHold(_) => sound.amplitude_mut().set_hold(unsigned).map_err(e),
                KitField::AmpDecay(_) => sound.amplitude_mut().set_decay(unsigned).map_err(e),
                KitField::AmpOverdrive(_) => {
                    sound.amplitude_mut().set_overdrive(unsigned).map_err(e)
                }
                KitField::AmpDelaySend(_) => {
                    sound.amplitude_mut().set_delay_send(unsigned).map_err(e)
                }
                KitField::AmpReverbSend(_) => {
                    sound.amplitude_mut().set_reverb_send(unsigned).map_err(e)
                }
                KitField::AmpPan(_) => sound.amplitude_mut().set_pan(signed).map_err(e),
                KitField::AmpVolume(_) => sound.amplitude_mut().set_volume(unsigned).map_err(e),
                KitField::FltAttack(_) => sound.filter_mut().set_attack(unsigned).map_err(e),
                KitField::FltSustain(_) => sound.filter_mut().set_sustain(unsigned).map_err(e),
                KitField::FltDecay(_) => sound.filter_mut().set_decay(unsigned).map_err(e),
                KitField::FltRelease(_) => sound.filter_mut().set_release(unsigned).map_err(e),
                KitField::FltCutoff(_) => sound.filter_mut().set_cutoff(unsigned).map_err(e),
                KitField::FltResonance(_) => sound.filter_mut().set_resonance(unsigned).map_err(e),
                KitField::FltEnvAmount(_) => {
                    sound.filter_mut().set_envelope_amount(signed).map_err(e)
                }
                _ => Err(format!("{} does not take an integer", field.label())),
            }
        }
        _ => Err(format!(
            "wrong value type for {} ({})",
            field.label(),
            edit.value
        )),
    }
}

/// Reads a field's current value out of an in-memory kit.
pub fn extract(field: KitField, kit: &Kit) -> Result<FieldValue, String> {
    let value = match field {
        KitField::KitName => FieldValue::Text(kit.name().to_owned()),
        KitField::SoundName(t) => FieldValue::Text(sound(kit, t)?.name().to_owned()),
        KitField::TrackLevel(t) => {
            FieldValue::Int(kit.track_level(t).map_err(|e| e.to_string())? as i32)
        }
        KitField::SampleTune(t) => FieldValue::Int(sound(kit, t)?.sample().tune() as i32),
        KitField::SampleFineTune(t) => FieldValue::Int(sound(kit, t)?.sample().fine_tune() as i32),
        KitField::SampleBitReduction(t) => {
            FieldValue::Int(sound(kit, t)?.sample().bit_reduction() as i32)
        }
        KitField::SampleStart(t) => FieldValue::Float(sound(kit, t)?.sample().start()),
        KitField::SampleEnd(t) => FieldValue::Float(sound(kit, t)?.sample().end()),
        KitField::SampleLoop(t) => FieldValue::Toggle(sound(kit, t)?.sample().loop_flag()),
        KitField::SampleLevel(t) => FieldValue::Int(sound(kit, t)?.sample().volume() as i32),
        KitField::AmpAttack(t) => FieldValue::Int(sound(kit, t)?.amplitude().attack() as i32),
        KitField::AmpHold(t) => FieldValue::Int(sound(kit, t)?.amplitude().hold() as i32),
        KitField::AmpDecay(t) => FieldValue::Int(sound(kit, t)?.amplitude().decay() as i32),
        KitField::AmpOverdrive(t) => FieldValue::Int(sound(kit, t)?.amplitude().overdrive() as i32),
        KitField::AmpDelaySend(t) => {
            FieldValue::Int(sound(kit, t)?.amplitude().delay_send() as i32)
        }
        KitField::AmpReverbSend(t) => {
            FieldValue::Int(sound(kit, t)?.amplitude().reverb_send() as i32)
        }
        // rytm-rs's pan() getter sign-extends the i8 field into usize;
        // truncate back to one byte to recover the signed value.
        KitField::AmpPan(t) => {
            FieldValue::Int(i32::from((sound(kit, t)?.amplitude().pan() as u8) as i8))
        }
        KitField::AmpVolume(t) => FieldValue::Int(sound(kit, t)?.amplitude().volume() as i32),
        KitField::FltAttack(t) => FieldValue::Int(sound(kit, t)?.filter().attack() as i32),
        KitField::FltSustain(t) => FieldValue::Int(sound(kit, t)?.filter().sustain() as i32),
        KitField::FltDecay(t) => FieldValue::Int(sound(kit, t)?.filter().decay() as i32),
        KitField::FltRelease(t) => FieldValue::Int(sound(kit, t)?.filter().release() as i32),
        KitField::FltCutoff(t) => FieldValue::Int(sound(kit, t)?.filter().cutoff() as i32),
        KitField::FltResonance(t) => FieldValue::Int(sound(kit, t)?.filter().resonance() as i32),
        KitField::FltEnvAmount(t) => {
            FieldValue::Int(sound(kit, t)?.filter().envelope_amount() as i32)
        }
    };
    Ok(value)
}

fn sound(kit: &Kit, track: usize) -> Result<&rytm_rs::object::Sound, String> {
    kit.sounds()
        .get(track)
        .ok_or_else(|| format!("no track {track}"))
}

fn sound_mut(kit: &mut Kit, track: usize) -> Result<&mut rytm_rs::object::Sound, String> {
    kit.sounds_mut()
        .get_mut(track)
        .ok_or_else(|| format!("no track {track}"))
}
