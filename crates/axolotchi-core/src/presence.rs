//! The presence state machine: `Present -> Missed(n) -> Gone`.
//!
//! A device regrows straight back to `Present` on any sighting, whether the
//! sighting came from an active ARP sweep or a passive sniff — a passive
//! sighting can rescue a device that active probing alone would have let go
//! missing. Only presence *transitions* (and IP changes) are state changes
//! worth persisting; incrementing a miss count is not, per the "only write
//! state changes" rule in `CLAUDE.md` — raw per-sweep sightings stay in the
//! caller's in-memory ring buffer.

use crate::{DeviceId, Effect, Timestamp};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    Present,
    Missed(u32),
    Gone,
}

/// Where a sighting came from: an active ARP sweep confirming a reply, or a
/// passive `AF_PACKET` sniff of traffic the device sent unprompted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SightingSource {
    Active,
    Passive,
}

#[derive(Debug, Clone)]
pub struct Device {
    pub id: DeviceId,
    pub ip: Option<String>,
    pub presence: Presence,
    pub last_seen: Timestamp,
    pub last_source: SightingSource,
    sighted_this_sweep: bool,
}

/// How many consecutive missed sweeps a device tolerates before it's
/// considered `Gone`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PresenceLimits {
    pub miss_limit: u32,
}

impl Default for PresenceLimits {
    fn default() -> Self {
        Self { miss_limit: 3 }
    }
}

/// Handles `Event::Sighting`: creates the device on first contact, or
/// regrows an existing one straight back to `Present` and updates its IP if
/// the sighting carried a new one.
pub fn on_sighting(
    devices: &mut HashMap<DeviceId, Device>,
    device_id: DeviceId,
    ip: Option<String>,
    source: SightingSource,
    at: Timestamp,
) -> Vec<Effect> {
    let mut effects = Vec::new();

    match devices.get_mut(&device_id) {
        None => {
            devices.insert(
                device_id.clone(),
                Device {
                    id: device_id.clone(),
                    ip,
                    presence: Presence::Present,
                    last_seen: at,
                    last_source: source,
                    sighted_this_sweep: true,
                },
            );
            effects.push(Effect::Persist { device_id });
            effects.push(Effect::Render);
        }
        Some(device) => {
            let mut changed = device.presence != Presence::Present;

            device.presence = Presence::Present;
            device.last_seen = at;
            device.last_source = source;
            device.sighted_this_sweep = true;

            if let Some(new_ip) = ip {
                if device.ip.as_deref() != Some(new_ip.as_str()) {
                    device.ip = Some(new_ip);
                    changed = true;
                }
            }

            if changed {
                effects.push(Effect::Persist { device_id });
                effects.push(Effect::Render);
            }
        }
    }

    effects
}

/// Handles `Event::SweepComplete`: any device not sighted during the sweep
/// that just finished takes one more miss, going `Gone` once it exceeds
/// `limits.miss_limit`. Devices sighted during the sweep just have their
/// per-sweep flag cleared for the next cycle.
pub fn on_sweep_complete(
    devices: &mut HashMap<DeviceId, Device>,
    limits: PresenceLimits,
) -> Vec<Effect> {
    let mut effects = Vec::new();

    for device in devices.values_mut() {
        if device.sighted_this_sweep {
            device.sighted_this_sweep = false;
            continue;
        }

        let missed = match device.presence {
            Presence::Present => Some(1),
            Presence::Missed(n) => Some(n + 1),
            Presence::Gone => None,
        };

        // A single limit check covers the first miss too, so a `miss_limit`
        // of 1 goes straight to `Gone` instead of pausing at `Missed(1)`.
        if let Some(missed) = missed {
            if missed >= limits.miss_limit {
                device.presence = Presence::Gone;
                effects.push(Effect::Persist {
                    device_id: device.id.clone(),
                });
                effects.push(Effect::Render);
            } else {
                let was_present = device.presence == Presence::Present;
                device.presence = Presence::Missed(missed);
                if was_present {
                    effects.push(Effect::Persist {
                        device_id: device.id.clone(),
                    });
                    effects.push(Effect::Render);
                }
            }
        }
    }

    effects
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sweep_n_times(devices: &mut HashMap<DeviceId, Device>, limits: PresenceLimits, n: u32) {
        for _ in 0..n {
            on_sweep_complete(devices, limits);
        }
    }

    #[test]
    fn limit_marks_device_gone_after_miss_limit_consecutive_sweeps() {
        let mut devices = HashMap::new();
        let limits = PresenceLimits { miss_limit: 3 };
        on_sighting(
            &mut devices,
            "dev-1".into(),
            None,
            SightingSource::Active,
            0,
        );
        // Clear the "sighted this sweep" flag from creation before missing.
        on_sweep_complete(&mut devices, limits);
        assert_eq!(devices["dev-1"].presence, Presence::Present);

        // Two more misses: Present -> Missed(1) -> Missed(2), still not Gone.
        sweep_n_times(&mut devices, limits, 2);
        assert_eq!(devices["dev-1"].presence, Presence::Missed(2));

        // Third consecutive miss reaches the limit.
        let effects = on_sweep_complete(&mut devices, limits);
        assert_eq!(devices["dev-1"].presence, Presence::Gone);
        assert_eq!(
            effects,
            vec![
                Effect::Persist {
                    device_id: "dev-1".into()
                },
                Effect::Render,
            ]
        );
    }

    #[test]
    fn regrow_resets_a_missed_device_straight_back_to_present() {
        let mut devices = HashMap::new();
        let limits = PresenceLimits { miss_limit: 3 };
        on_sighting(
            &mut devices,
            "dev-1".into(),
            None,
            SightingSource::Active,
            0,
        );
        on_sweep_complete(&mut devices, limits);
        sweep_n_times(&mut devices, limits, 2);
        assert_eq!(devices["dev-1"].presence, Presence::Missed(2));

        let effects = on_sighting(
            &mut devices,
            "dev-1".into(),
            None,
            SightingSource::Active,
            100,
        );
        assert_eq!(devices["dev-1"].presence, Presence::Present);
        assert_eq!(devices["dev-1"].last_seen, 100);
        assert_eq!(
            effects,
            vec![
                Effect::Persist {
                    device_id: "dev-1".into()
                },
                Effect::Render,
            ]
        );

        // The miss count fully reset, not merely decremented: it takes a
        // fresh full run of misses to go Gone again.
        on_sweep_complete(&mut devices, limits);
        sweep_n_times(&mut devices, limits, 2);
        assert_eq!(devices["dev-1"].presence, Presence::Missed(2));
    }

    #[test]
    fn passive_sighting_rescues_a_device_on_the_verge_of_going_gone() {
        let mut devices = HashMap::new();
        let limits = PresenceLimits { miss_limit: 3 };
        on_sighting(
            &mut devices,
            "dev-1".into(),
            None,
            SightingSource::Active,
            0,
        );
        on_sweep_complete(&mut devices, limits);
        sweep_n_times(&mut devices, limits, 2);
        assert_eq!(devices["dev-1"].presence, Presence::Missed(2));

        // A passive sniff (no ARP reply needed) rescues it just as well.
        on_sighting(
            &mut devices,
            "dev-1".into(),
            None,
            SightingSource::Passive,
            150,
        );
        assert_eq!(devices["dev-1"].presence, Presence::Present);
        assert_eq!(devices["dev-1"].last_source, SightingSource::Passive);

        // The sweep right after a sighting just clears the per-sweep flag;
        // it takes a fresh miss after that to start counting again — proof
        // the rescue didn't leave it primed to go Gone on the next sweep.
        on_sweep_complete(&mut devices, limits);
        assert_eq!(devices["dev-1"].presence, Presence::Present);
        on_sweep_complete(&mut devices, limits);
        assert_eq!(devices["dev-1"].presence, Presence::Missed(1));
    }

    #[test]
    fn passive_sighting_also_rescues_a_device_that_already_went_gone() {
        let mut devices = HashMap::new();
        let limits = PresenceLimits { miss_limit: 1 };
        on_sighting(
            &mut devices,
            "dev-1".into(),
            None,
            SightingSource::Active,
            0,
        );
        on_sweep_complete(&mut devices, limits);
        on_sweep_complete(&mut devices, limits);
        assert_eq!(devices["dev-1"].presence, Presence::Gone);

        on_sighting(
            &mut devices,
            "dev-1".into(),
            None,
            SightingSource::Passive,
            200,
        );
        assert_eq!(devices["dev-1"].presence, Presence::Present);
    }

    #[test]
    fn ip_change_updates_address_and_persists_even_without_a_presence_change() {
        let mut devices = HashMap::new();
        let limits = PresenceLimits::default();
        on_sighting(
            &mut devices,
            "dev-1".into(),
            Some("10.0.0.5".into()),
            SightingSource::Active,
            0,
        );
        on_sweep_complete(&mut devices, limits);
        assert_eq!(devices["dev-1"].presence, Presence::Present);

        let effects = on_sighting(
            &mut devices,
            "dev-1".into(),
            Some("10.0.0.9".into()),
            SightingSource::Active,
            10,
        );
        assert_eq!(devices["dev-1"].ip.as_deref(), Some("10.0.0.9"));
        assert_eq!(
            effects,
            vec![
                Effect::Persist {
                    device_id: "dev-1".into()
                },
                Effect::Render,
            ]
        );
    }

    #[test]
    fn same_ip_resighted_while_present_is_a_no_op() {
        let mut devices = HashMap::new();
        on_sighting(
            &mut devices,
            "dev-1".into(),
            Some("10.0.0.5".into()),
            SightingSource::Active,
            0,
        );
        let effects = on_sighting(
            &mut devices,
            "dev-1".into(),
            Some("10.0.0.5".into()),
            SightingSource::Active,
            10,
        );
        assert!(effects.is_empty());
    }

    #[test]
    fn missing_ip_on_a_sighting_does_not_clear_a_known_ip() {
        let mut devices = HashMap::new();
        on_sighting(
            &mut devices,
            "dev-1".into(),
            Some("10.0.0.5".into()),
            SightingSource::Active,
            0,
        );
        // A passive sighting with no observed source IP shouldn't erase it.
        on_sighting(
            &mut devices,
            "dev-1".into(),
            None,
            SightingSource::Passive,
            10,
        );
        assert_eq!(devices["dev-1"].ip.as_deref(), Some("10.0.0.5"));
    }
}
