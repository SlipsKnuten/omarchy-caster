use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AudioMode {
    #[default]
    Off,
    Application,
    System,
}

impl fmt::Display for AudioMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Off => "off",
            Self::Application => "application",
            Self::System => "system",
        })
    }
}

impl FromStr for AudioMode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "off" => Ok(Self::Off),
            "application" => Ok(Self::Application),
            "system" => Ok(Self::System),
            _ => Err("expected off, application, or system".into()),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SourceKind {
    Screen,
    #[default]
    Window,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Source {
    pub id: String,
    pub kind: SourceKind,
    pub name: String,
    pub label: String,
    pub pid: u32,
    pub identity: String,
    pub app: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Receiver {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub model: String,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum QualityPreset {
    #[default]
    Auto,
    #[serde(rename = "720p")]
    P720,
    #[serde(rename = "1080p")]
    P1080,
}

impl fmt::Display for QualityPreset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Auto => "auto",
            Self::P720 => "720p",
            Self::P1080 => "1080p",
        })
    }
}

impl FromStr for QualityPreset {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "auto" => Ok(Self::Auto),
            "720p" => Ok(Self::P720),
            "1080p" => Ok(Self::P1080),
            _ => Err("expected auto, 720p, or 1080p".into()),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BitrateChoice {
    #[default]
    Auto,
    #[serde(rename = "4")]
    M4,
    #[serde(rename = "6")]
    M6,
    #[serde(rename = "8")]
    M8,
}

impl fmt::Display for BitrateChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Auto => "auto",
            Self::M4 => "4",
            Self::M6 => "6",
            Self::M8 => "8",
        })
    }
}

impl FromStr for BitrateChoice {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "auto" => Ok(Self::Auto),
            "4" => Ok(Self::M4),
            "6" => Ok(Self::M6),
            "8" => Ok(Self::M8),
            _ => Err("expected auto, 4, 6, or 8".into()),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Quality {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate: u32,
}

impl Quality {
    pub fn resolve(preset: QualityPreset, bitrate: BitrateChoice) -> Self {
        let (width, height, automatic_bitrate) = match preset {
            QualityPreset::P720 => (1280, 720, 4_000_000),
            QualityPreset::Auto | QualityPreset::P1080 => (1920, 1080, 6_000_000),
        };
        let bitrate = match bitrate {
            BitrateChoice::Auto => automatic_bitrate,
            BitrateChoice::M4 => 4_000_000,
            BitrateChoice::M6 => 6_000_000,
            BitrateChoice::M8 => 8_000_000,
        };
        Self {
            width,
            height,
            fps: 30,
            bitrate,
        }
    }
}

impl Default for Quality {
    fn default() -> Self {
        Self::resolve(QualityPreset::Auto, BitrateChoice::Auto)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Request {
    pub receiver: Receiver,
    pub source: Source,
    pub audio: AudioMode,
    #[serde(default)]
    pub quality: QualityPreset,
    #[serde(default)]
    pub bitrate: BitrateChoice,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SessionState {
    #[default]
    Idle,
    Discovering,
    Connecting,
    Negotiating,
    Streaming,
    Stopping,
    Error,
}

impl fmt::Display for SessionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let serialized = serde_json::to_value(self).map_err(|_| fmt::Error)?;
        f.write_str(serialized.as_str().ok_or(fmt::Error)?)
    }
}

impl SessionState {
    pub const fn can_transition_to(self, next: Self) -> bool {
        use SessionState::{
            Connecting, Discovering, Error, Idle, Negotiating, Stopping, Streaming,
        };
        self as u8 == next as u8
            || matches!(
                (self, next),
                (Idle, Discovering | Connecting | Error)
                    | (Discovering, Idle | Connecting | Error)
                    | (Connecting, Negotiating | Streaming | Stopping | Error)
                    | (Negotiating, Streaming | Stopping | Error)
                    | (Streaming, Stopping | Error)
                    | (Stopping, Idle | Error)
                    | (Error, Idle | Connecting)
            )
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Status {
    pub state: SessionState,
    pub detail: String,
    #[serde(default)]
    pub device: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub source_id: String,
    #[serde(default)]
    pub device_id: String,
    #[serde(default)]
    pub source_kind: Option<SourceKind>,
    #[serde(default)]
    pub audio_detail: String,
    #[serde(default)]
    pub scheduled_playout: String,
    #[serde(default)]
    pub resolved_quality: String,
}

impl Default for Status {
    fn default() -> Self {
        Self {
            state: SessionState::Idle,
            detail: "Ready".into(),
            device: String::new(),
            source: String::new(),
            source_id: String::new(),
            device_id: String::new(),
            source_kind: None,
            audio_detail: String::new(),
            scheduled_playout: String::new(),
            resolved_quality: String::new(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct Capabilities {
    pub receiver_discovery: Vec<String>,
    pub capture: Vec<String>,
    pub audio: Vec<String>,
    pub transport: Vec<String>,
    pub supervisor: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BackendKind {
    ReceiverDiscovery,
    Capture,
    Audio,
    Transport,
    Supervisor,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct BackendInfo {
    pub id: String,
    pub kind: BackendKind,
    pub compiled: bool,
    pub available: bool,
    pub detail: String,
    pub capabilities: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct CapabilityReport {
    pub backends: Vec<BackendInfo>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quality_presets_have_stable_limits() {
        assert_eq!(Quality::default().width, 1920);
        assert_eq!(
            Quality::resolve(QualityPreset::P720, BitrateChoice::Auto).bitrate,
            4_000_000
        );
        assert_eq!(
            Quality::resolve(QualityPreset::P1080, BitrateChoice::M8).bitrate,
            8_000_000
        );
    }

    #[test]
    fn public_enums_parse_and_serialize_consistently() {
        assert_eq!("application".parse(), Ok(AudioMode::Application));
        assert_eq!("1080p".parse(), Ok(QualityPreset::P1080));
        assert_eq!(
            serde_json::to_string(&SessionState::Streaming).unwrap(),
            "\"streaming\""
        );
    }

    #[test]
    fn session_transitions_are_explicit_and_bounded() {
        use SessionState::*;
        let states = [
            Idle,
            Discovering,
            Connecting,
            Negotiating,
            Streaming,
            Stopping,
            Error,
        ];
        let allowed = [
            (Idle, Discovering),
            (Idle, Connecting),
            (Idle, Error),
            (Discovering, Idle),
            (Discovering, Connecting),
            (Discovering, Error),
            (Connecting, Negotiating),
            (Connecting, Streaming),
            (Connecting, Stopping),
            (Connecting, Error),
            (Negotiating, Streaming),
            (Negotiating, Stopping),
            (Negotiating, Error),
            (Streaming, Stopping),
            (Streaming, Error),
            (Stopping, Idle),
            (Stopping, Error),
            (Error, Idle),
            (Error, Connecting),
        ];
        for from in states {
            for to in states {
                assert_eq!(
                    from.can_transition_to(to),
                    from == to || allowed.contains(&(from, to)),
                    "unexpected transition {from:?} -> {to:?}"
                );
            }
        }
    }
}
