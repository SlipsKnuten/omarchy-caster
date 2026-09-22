//! Pulse-compatible PipeWire routing, isolated by process ancestry and start time.
use crate::{
    capture::{Source, json_command, process_identity},
    storage,
};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, clap::ValueEnum, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Off,
    Application,
    System,
}
#[derive(Deserialize, Serialize)]
struct Route {
    sink: String,
    identity: String,
}
#[derive(Deserialize, Serialize)]
struct Routes {
    name: String,
    routes: BTreeMap<String, Route>,
}

fn pactl(args: &[&str]) -> Result<String> {
    let out = Command::new("pactl").args(args).output()?;
    anyhow::ensure!(
        out.status.success(),
        "Audio routing: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(String::from_utf8(out.stdout)?.trim().to_owned())
}
fn rows(kind: &str) -> Result<Vec<serde_json::Value>> {
    let json = json_command("pactl", &["-f", "json", "list", kind])?;
    Ok(json.as_array().context("Malformed audio graph")?.clone())
}
fn input_pid(row: &serde_json::Value) -> Option<u32> {
    row["properties"]["application.process.id"]
        .as_str()?
        .parse()
        .ok()
}
pub fn belongs_to(mut pid: u32, owner: u32, identity: &str) -> bool {
    if process_identity(owner).as_deref() != Some(identity) {
        return false;
    }
    let mut seen = HashSet::new();
    while pid > 1 && seen.insert(pid) {
        if pid == owner {
            return true;
        }
        let Ok(stat) = fs::read_to_string(format!("/proc/{pid}/stat")) else {
            return false;
        };
        let Some((_, fields)) = stat.rsplit_once(')') else {
            return false;
        };
        let Some(parent) = fields
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
        else {
            return false;
        };
        pid = parent;
    }
    false
}
pub fn default_monitor() -> Result<String> {
    Ok(format!("{}.monitor", pactl(&["get-default-sink"])?))
}
pub fn recover() -> Result<()> {
    let path = storage::directory("runtime")?.join("audio.json");
    if !path.exists() {
        return Ok(());
    }
    let state: Routes = storage::read(&path)?;
    let sinks = rows("sinks")?;
    let own = sinks
        .iter()
        .find(|s| s["name"].as_str() == Some(&state.name));
    if let Some(own) = own {
        for row in rows("sink-inputs")? {
            let Some(index) = row["index"].as_u64() else {
                continue;
            };
            let Some(previous) = state.routes.get(&index.to_string()) else {
                continue;
            };
            if row["sink"] != own["index"] {
                continue;
            }
            if input_pid(&row).and_then(process_identity).as_deref() != Some(&previous.identity) {
                continue;
            }
            let target = if sinks
                .iter()
                .any(|s| s["name"].as_str() == Some(&previous.sink))
            {
                previous.sink.clone()
            } else {
                pactl(&["get-default-sink"])?
            };
            pactl(&["move-sink-input", &index.to_string(), &target])?;
        }
    }
    let exact_arg = format!("sink_name={}", state.name);
    // PipeWire's pactl JSON module list may omit index; the short format includes it.
    for line in pactl(&["list", "short", "modules"])?.lines() {
        let columns: Vec<_> = line.split('\t').collect();
        if columns.len() >= 3
            && columns[1] == "module-null-sink"
            && columns[2].split_whitespace().any(|arg| arg == exact_arg)
        {
            let id: u32 = columns[0].parse().context("Invalid module index")?;
            pactl(&["unload-module", &id.to_string()])?;
        }
    }
    fs::remove_file(path)?;
    Ok(())
}
pub struct AppAudio {
    source: Source,
    state: Routes,
}
impl AppAudio {
    pub fn new(source: Source) -> Result<Self> {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let state = Routes {
            name: format!("omacast_dev_{}_{nonce}", std::process::id()),
            routes: BTreeMap::new(),
        };
        storage::write(&storage::directory("runtime")?.join("audio.json"), &state)?;
        pactl(&[
            "load-module",
            "module-null-sink",
            &format!("sink_name={}", state.name),
            "sink_properties=device.description=OmaCast-Application",
            "rate=48000",
            "channels=2",
        ])?;
        Ok(Self { source, state })
    }
    pub fn monitor(&self) -> String {
        format!("{}.monitor", self.state.name)
    }
    pub fn update(&mut self) -> Result<String> {
        anyhow::ensure!(
            process_identity(self.source.pid).as_deref() == Some(&self.source.identity),
            "Selected application exited"
        );
        let sinks = rows("sinks")?;
        let own = sinks
            .iter()
            .find(|s| s["name"].as_str() == Some(&self.state.name))
            .context("Application audio sink disappeared")?;
        let mut matched = 0;
        for row in rows("sink-inputs")? {
            let Some(pid) = input_pid(&row) else {
                continue;
            };
            if !belongs_to(pid, self.source.pid, &self.source.identity) {
                continue;
            }
            matched += 1;
            if row["sink"] == own["index"] {
                continue;
            }
            let Some(index) = row["index"].as_u64() else {
                continue;
            };
            let Some(previous) = sinks
                .iter()
                .find(|s| s["index"] == row["sink"])
                .and_then(|s| s["name"].as_str())
            else {
                continue;
            };
            let identity = process_identity(pid).context("Audio application disappeared")?;
            self.state.routes.insert(
                index.to_string(),
                Route {
                    sink: previous.to_owned(),
                    identity,
                },
            );
            storage::write(
                &storage::directory("runtime")?.join("audio.json"),
                &self.state,
            )?;
            pactl(&["move-sink-input", &index.to_string(), &self.state.name])?;
        }
        Ok(if matched > 0 {
            "Application audio → TV"
        } else {
            "No matching app audio; sending silence"
        }
        .to_owned())
    }
}
pub fn command(monitor: &str) -> Command {
    let mut cmd = Command::new("ffmpeg");
    cmd.args([
        "-nostdin",
        "-hide_banner",
        "-loglevel",
        "error",
        "-f",
        "pulse",
        "-fragment_size",
        "3840",
        "-i",
        monitor,
        "-ac",
        "2",
        "-ar",
        "48000",
        "-c:a",
        "pcm_f32le",
        "-flush_packets",
        "1",
        "-f",
        "nut",
        "pipe:1",
    ]);
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires a live Pulse-compatible audio session and ffplay; creates two silent test streams"]
    fn live_routing_isolates_one_process_and_restores_its_output() {
        use std::{
            process::{Child, Stdio},
            thread,
            time::{Duration, Instant},
        };
        struct Cleanup(Vec<Child>);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = recover();
                for child in &mut self.0 {
                    let _ = child.kill();
                    let _ = child.wait();
                }
            }
        }
        let state = storage::directory("runtime").unwrap().join("audio.json");
        assert!(
            !state.exists(),
            "Do not run while application audio is being cast"
        );
        let mut cleanup = Cleanup(Vec::new());
        for frequency in [440, 880] {
            cleanup.0.push(
                Command::new("ffplay")
                    .env("SDL_AUDIO_DRIVER", "pulseaudio")
                    .env("SDL_AUDIODRIVER", "pulseaudio")
                    .args([
                        "-nodisp",
                        "-volume",
                        "0",
                        "-loglevel",
                        "error",
                        "-f",
                        "lavfi",
                        "-i",
                        &format!("sine=frequency={frequency}:sample_rate=48000"),
                    ])
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .unwrap(),
            );
        }
        let selected = cleanup.0[0].id();
        let other = cleanup.0[1].id();
        let deadline = Instant::now() + Duration::from_secs(5);
        let before = loop {
            let inputs = rows("sink-inputs").unwrap();
            if inputs.iter().any(|r| input_pid(r) == Some(selected))
                && inputs.iter().any(|r| input_pid(r) == Some(other))
            {
                break inputs;
            }
            assert!(
                Instant::now() < deadline,
                "Silent test audio streams did not appear"
            );
            thread::sleep(Duration::from_millis(100));
        };
        let selected_before = before
            .iter()
            .find(|r| input_pid(r) == Some(selected))
            .unwrap()["sink"]
            .clone();
        let other_before =
            before.iter().find(|r| input_pid(r) == Some(other)).unwrap()["sink"].clone();
        let source = Source {
            id: "test".into(),
            kind: crate::capture::Kind::Window,
            name: "test".into(),
            label: "silent test".into(),
            pid: selected,
            identity: process_identity(selected).unwrap(),
            app: "ffplay".into(),
        };
        let mut routing = AppAudio::new(source).unwrap();
        assert_eq!(routing.update().unwrap(), "Application audio → TV");
        let sinks = rows("sinks").unwrap();
        let own = sinks
            .iter()
            .find(|s| s["name"].as_str() == Some(&routing.state.name))
            .unwrap()["index"]
            .clone();
        let during = rows("sink-inputs").unwrap();
        assert_eq!(
            during
                .iter()
                .find(|r| input_pid(r) == Some(selected))
                .unwrap()["sink"],
            own
        );
        assert_eq!(
            during.iter().find(|r| input_pid(r) == Some(other)).unwrap()["sink"],
            other_before
        );
        drop(routing); // Recovery must work from disk, as ExecStopPost does after a crash.
        recover().unwrap();
        let after = rows("sink-inputs").unwrap();
        assert_eq!(
            after
                .iter()
                .find(|r| input_pid(r) == Some(selected))
                .unwrap()["sink"],
            selected_before
        );
        assert_eq!(
            after.iter().find(|r| input_pid(r) == Some(other)).unwrap()["sink"],
            other_before
        );
        assert!(!state.exists());
    }
    #[test]
    fn own_process_matches_but_recycled_identity_does_not() {
        let pid = std::process::id();
        assert!(belongs_to(pid, pid, &process_identity(pid).unwrap()));
        assert!(!belongs_to(pid, pid, "stale"));
        assert!(!belongs_to(0, pid, &process_identity(pid).unwrap()));
    }
    #[test]
    fn missing_pid_is_not_a_name_based_match() {
        assert_eq!(
            input_pid(&serde_json::json!({"properties":{"application.name":"Zen"}})),
            None
        );
    }
}
