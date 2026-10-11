//! Declarative receiver compatibility data. Profiles are fallback limits;
//! negotiated transport constraints take precedence when available.

use crate::{BitrateChoice, Quality, QualityPreset, Receiver};
use serde::{Deserialize, Serialize};

const PROFILE_DATA: &str = include_str!("../data/receiver-profiles.json");

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ReceiverProfile {
    pub id: String,
    pub models: Vec<String>,
    pub max_width: u32,
    pub max_height: u32,
    pub max_fps: u32,
    pub recommended_bitrate: u32,
    pub max_bitrate: u32,
}

pub fn profiles() -> Result<Vec<ReceiverProfile>, serde_json::Error> {
    serde_json::from_str(PROFILE_DATA)
}

pub fn profile_for(receiver: &Receiver) -> ReceiverProfile {
    let profiles = profiles().expect("bundled receiver profiles must be valid");
    profiles
        .iter()
        .skip(1)
        .find(|profile| {
            profile
                .models
                .iter()
                .any(|model| model.eq_ignore_ascii_case(&receiver.model))
        })
        .cloned()
        .unwrap_or_else(|| profiles[0].clone())
}

pub fn resolve_quality(
    receiver: &Receiver,
    preset: QualityPreset,
    bitrate: BitrateChoice,
) -> (ReceiverProfile, Quality) {
    let profile = profile_for(receiver);
    let mut quality = Quality::resolve(preset, bitrate);
    quality.width = quality.width.min(profile.max_width);
    quality.height = quality.height.min(profile.max_height);
    // Each profile's maximum is verified on hardware, so use it by default.
    quality.fps = profile.max_fps;
    quality.bitrate = match bitrate {
        BitrateChoice::Auto => profile.recommended_bitrate,
        _ => quality.bitrate.min(profile.max_bitrate),
    };
    (profile, quality)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn receiver(model: &str) -> Receiver {
        Receiver {
            id: "stable-id".into(),
            name: "Friendly name must not select a profile".into(),
            host: "192.0.2.1".into(),
            port: 8009,
            model: model.into(),
        }
    }

    #[test]
    fn every_profile_has_safe_limits() {
        let profiles = profiles().unwrap();
        assert!(!profiles.is_empty());
        for profile in profiles {
            assert!(profile.max_width <= 1920);
            assert!(profile.max_height <= 1080);
            assert!(profile.max_fps <= 60);
            assert!(profile.recommended_bitrate <= profile.max_bitrate);
            assert!(profile.max_bitrate <= 8_000_000);
        }
    }

    #[test]
    fn selection_uses_model_not_friendly_name() {
        assert_eq!(
            profile_for(&receiver("4K Google TV Stick")).id,
            "google-tv-stick-4k"
        );
        assert_eq!(
            profile_for(&receiver("Unknown model")).id,
            "google-cast-default"
        );
    }

    #[test]
    fn chromecast_ultra_streams_at_60_fps_others_at_30() {
        let ultra =
            |model| resolve_quality(&receiver(model), QualityPreset::Auto, BitrateChoice::Auto).1;
        assert_eq!(ultra("Chromecast Ultra").fps, 60);
        assert_eq!(ultra("Unknown model").fps, 30);
        assert_eq!(ultra("4K Google TV Stick").fps, 30);
    }

    #[test]
    fn manual_bitrate_is_clamped_to_profile() {
        let (_, quality) = resolve_quality(
            &receiver("Unknown model"),
            QualityPreset::P720,
            BitrateChoice::M8,
        );
        assert_eq!(quality.width, 1280);
        assert_eq!(quality.bitrate, 8_000_000);
    }
}
