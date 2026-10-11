//! Compile-time extension points. Implementations live in focused adapter crates.

use crate::{AudioMode, Capabilities, Quality, Receiver, Source, Status};

pub type AdapterResult<T> = Result<T, AdapterError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterError {
    pub code: &'static str,
    pub message: String,
}

impl AdapterError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

pub trait ReceiverDiscovery: Send + Sync {
    fn id(&self) -> &'static str;
    fn discover(&self) -> AdapterResult<Vec<Receiver>>;
}

pub trait CaptureBackend: Send + Sync {
    fn id(&self) -> &'static str;
    fn capabilities(&self) -> Capabilities;
    fn sources(&self) -> AdapterResult<Vec<Source>>;
    fn validate(&self, source: &Source) -> AdapterResult<()>;
}

pub trait AudioBackend: Send + Sync {
    fn id(&self) -> &'static str;
    fn supports(&self, source: &Source, mode: AudioMode) -> bool;
    fn recover(&self) -> AdapterResult<()>;
}

pub trait TransportBackend: Send + Sync {
    fn id(&self) -> &'static str;
    fn supports(&self, receiver: &Receiver) -> bool;
    fn validate_quality(&self, quality: Quality) -> AdapterResult<()>;
}

pub trait SessionSupervisor: Send + Sync {
    fn id(&self) -> &'static str;
    fn is_active(&self) -> AdapterResult<bool>;
    fn stop(&self) -> AdapterResult<Status>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SessionState, SourceKind};

    struct FakeCapture;

    struct FakeDiscovery;

    impl ReceiverDiscovery for FakeDiscovery {
        fn id(&self) -> &'static str {
            "fake-discovery"
        }

        fn discover(&self) -> AdapterResult<Vec<Receiver>> {
            Ok(vec![fixture_receiver()])
        }
    }

    struct FakeAudio;

    impl AudioBackend for FakeAudio {
        fn id(&self) -> &'static str {
            "fake-audio"
        }

        fn supports(&self, source: &Source, mode: AudioMode) -> bool {
            mode == AudioMode::Off
                || (source.kind == SourceKind::Window && mode == AudioMode::Application)
        }

        fn recover(&self) -> AdapterResult<()> {
            Ok(())
        }
    }

    impl CaptureBackend for FakeCapture {
        fn id(&self) -> &'static str {
            "fake-capture"
        }

        fn capabilities(&self) -> Capabilities {
            Capabilities {
                capture: vec!["window".into()],
                ..Capabilities::default()
            }
        }

        fn sources(&self) -> AdapterResult<Vec<Source>> {
            Ok(vec![Source {
                id: "fake:1".into(),
                kind: SourceKind::Window,
                name: "opaque-handle".into(),
                label: "Fixture window".into(),
                pid: 42,
                identity: "42:1".into(),
                app: "fixture".into(),
            }])
        }

        fn validate(&self, source: &Source) -> AdapterResult<()> {
            if source.id == "fake:1" {
                Ok(())
            } else {
                Err(AdapterError::new(
                    "capture.source_changed",
                    "fixture source disappeared",
                ))
            }
        }
    }

    struct FakeTransport;

    impl TransportBackend for FakeTransport {
        fn id(&self) -> &'static str {
            "fake-transport"
        }

        fn supports(&self, receiver: &Receiver) -> bool {
            receiver.model == "Fixture"
        }

        fn validate_quality(&self, quality: Quality) -> AdapterResult<()> {
            if quality.width <= 1920 {
                Ok(())
            } else {
                Err(AdapterError::new("quality.unsupported", "fixture limit"))
            }
        }
    }

    struct FakeSupervisor;

    fn fixture_receiver() -> Receiver {
        Receiver {
            id: "fixture-id".into(),
            name: "Room name is display-only".into(),
            host: "192.0.2.1".into(),
            port: 8009,
            model: "Fixture".into(),
        }
    }

    impl SessionSupervisor for FakeSupervisor {
        fn id(&self) -> &'static str {
            "fake-supervisor"
        }

        fn is_active(&self) -> AdapterResult<bool> {
            Ok(true)
        }

        fn stop(&self) -> AdapterResult<Status> {
            Ok(Status {
                state: SessionState::Idle,
                detail: "fixture stopped".into(),
                ..Status::default()
            })
        }
    }

    #[test]
    fn fake_adapters_lock_extension_contracts() {
        let capture = FakeCapture;
        let source = capture.sources().unwrap().remove(0);
        assert_eq!(capture.id(), "fake-capture");
        assert_eq!(capture.capabilities().capture, ["window"]);
        assert!(capture.validate(&source).is_ok());

        let discovery = FakeDiscovery;
        assert_eq!(discovery.discover().unwrap(), [fixture_receiver()]);

        let audio = FakeAudio;
        assert!(audio.supports(&source, AudioMode::Application));
        assert!(!audio.supports(&source, AudioMode::System));
        assert!(audio.recover().is_ok());

        let transport = FakeTransport;
        let receiver = fixture_receiver();
        assert!(transport.supports(&receiver));
        assert!(transport.validate_quality(Quality::default()).is_ok());

        let supervisor = FakeSupervisor;
        assert!(supervisor.is_active().unwrap());
        assert_eq!(supervisor.stop().unwrap().state, SessionState::Idle);
    }
}
