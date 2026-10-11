import QtQuick
import Quickshell
import Quickshell.Io
import "Model.js" as Model

Item {
  id: service

  property string executable: "caster"
  property var receivers: []
  property var sources: []
  property var session: ({ state: "idle", detail: "Ready" })
  property string error: ""
  property string diagnostics: ""
  property var preferences: ({})
  property bool preferencesLoaded: false
  // Setup state: the plugin is cloned by `omarchy plugin add`, but the `caster`
  // binary and patched sender are installed separately. Surface that plainly
  // instead of showing an empty, apparently broken panel.
  property bool installChecked: false
  property bool installed: false
  property var setupIssues: []
  property bool scanRequested: false
  readonly property bool ready: installChecked && installed && setupIssues.length === 0
  property bool scanning: receiverProcess.running || sourceProcess.running
  property bool busy: actionProcess.running
  readonly property string runtimeHome: Quickshell.env("XDG_RUNTIME_DIR")
  readonly property string statusPath: runtimeHome === "" ? "" : runtimeHome + "/caster/status.json"

  function accept(text, callback) {
    try {
      var result = Model.envelope(text)
      error = ""
      callback(result)
    } catch (exception) {
      error = String(exception).replace(/^Error: /, "")
    }
  }

  // Re-check that the command exists, then scan. The scan is skipped when the
  // command is missing so nothing logs a failed spawn.
  function refresh() {
    scanRequested = true
    probe()
  }

  function probe() {
    if (!probeProcess.running) probeProcess.running = true
  }

  function scan() {
    if (!receiverProcess.running) receiverProcess.running = true
    if (!sourceProcess.running) sourceProcess.running = true
    if ((!preferencesLoaded || setupIssues.length > 0) && !doctorProcess.running)
      doctorProcess.running = true
  }

  function refreshStatus() {
    if (statusPath !== "") statusFile.reload()
    if (installed && !statusProcess.running) statusProcess.running = true
  }

  function start(receiver, source, audio, quality, bitrate) {
    if (busy || !ready || !receiver || !source) return
    error = ""
    actionProcess.command = [executable, "--json", "start",
      "--device", receiver.id, "--source", source.id,
      "--audio", audio, "--quality", quality, "--bitrate", bitrate]
    actionProcess.running = true
  }

  function stop() {
    if (busy) return
    error = ""
    actionProcess.command = [executable, "--json", "stop"]
    actionProcess.running = true
  }

  function runDoctor() {
    if (installed && !doctorProcess.running) doctorProcess.running = true
  }

  function loadStatus(text) {
    try {
      var value = JSON.parse(String(text || ""))
      if (value && value.state) session = value
    } catch (exception) {
      // Atomic writes normally prevent partial reads; retry once if the
      // compositor delivered a coalesced event at the rename boundary.
      statusRetry.restart()
    }
  }

  Component.onCompleted: {
    refreshStatus()
    probe()
  }

  Process {
    id: probeProcess
    // The name is passed as an argument, never interpolated into the script.
    command: ["sh", "-c", "command -v \"$1\" >/dev/null 2>&1", "sh", service.executable]
    onExited: function(exitCode) {
      service.installed = exitCode === 0
      service.installChecked = true
      if (!service.installed) {
        service.setupIssues = ["The \u201c" + service.executable + "\u201d command was not found on your PATH."]
        service.receivers = []
        service.sources = []
        service.error = ""
        service.scanRequested = false
        return
      }
      service.refreshStatus()
      if (service.scanRequested) {
        service.scanRequested = false
        service.scan()
      }
    }
  }

  FileView {
    id: statusFile
    path: service.statusPath
    watchChanges: true
    atomicWrites: true
    printErrors: false
    onLoaded: service.loadStatus(text())
    onFileChanged: reload()
  }

  Timer {
    id: statusRetry
    interval: 100
    repeat: false
    onTriggered: statusFile.reload()
  }

  Process {
    id: statusProcess
    command: [service.executable, "--json", "status"]
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: service.accept(text, function(value) { service.session = value })
    }
  }

  Process {
    id: receiverProcess
    command: [service.executable, "--json", "devices"]
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: service.accept(text, function(value) { service.receivers = value })
    }
  }

  Process {
    id: sourceProcess
    command: [service.executable, "--json", "sources"]
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: service.accept(text, function(value) { service.sources = value })
    }
  }

  Process {
    id: actionProcess
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: service.accept(text, function(value) { service.session = value })
    }
    onExited: service.refreshStatus()
  }

  Process {
    id: doctorProcess
    command: [service.executable, "--json", "doctor"]
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: service.accept(text, function(value) {
        var unavailable = value.capabilities.backends.filter(function(backend) { return !backend.available })
        service.diagnostics = unavailable.length === 0
          ? "All compiled backends are available."
          : unavailable.map(function(backend) { return backend.id + ": " + backend.detail }).join("\n")
        // Audio is optional (screen and video-only casts still work); every
        // other backend is required to start a session.
        service.setupIssues = unavailable
          .filter(function(backend) { return backend.kind !== "audio" })
          .map(function(backend) { return backend.detail || backend.id })
        service.preferencesLoaded = true
        service.preferences = value.config.preferences || ({})
      })
    }
  }
}
