//! Google Cast discovery, independent of UI and capture backend.
use anyhow::Result;
pub use caster_core::Receiver;
use mdns_sd::{ServiceDaemon, ServiceEvent};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

const CAST_SERVICE: &str = "_googlecast._tcp.local.";
pub fn receivers() -> Result<Vec<Receiver>> {
    let daemon = ServiceDaemon::new()?;
    let events = daemon.browse(CAST_SERVICE)?;
    let end = Instant::now() + Duration::from_secs(4);
    let mut found = BTreeMap::new();
    while Instant::now() < end {
        match events.recv_timeout(end.saturating_duration_since(Instant::now())) {
            Ok(ServiceEvent::ServiceResolved(info)) => {
                // Prefer IPv4 on the LAN; scoped IPv6 needs interface-aware endpoints.
                let host = info
                    .get_addresses()
                    .iter()
                    .filter(|a| a.is_ipv4())
                    .min()
                    .map(ToString::to_string);
                if let (Some(host), Some(id)) = (host, info.get_property_val_str("id")) {
                    found.insert(
                        id.to_owned(),
                        Receiver {
                            id: id.to_owned(),
                            name: info
                                .get_property_val_str("fn")
                                .unwrap_or(info.get_fullname())
                                .to_owned(),
                            host,
                            port: info.get_port(),
                            model: info
                                .get_property_val_str("md")
                                .unwrap_or("Unknown")
                                .to_owned(),
                        },
                    );
                }
            }
            Err(_) => break,
            _ => {}
        }
    }
    let _ = daemon.stop_browse(CAST_SERVICE);
    let _ = daemon.shutdown();
    let mut result: Vec<_> = found.into_values().collect();
    result.sort_by_key(|r| r.name.to_lowercase());
    Ok(result)
}
