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

  function refresh() {
    if (!receiverProcess.running) receiverProcess.running = true
    if (!sourceProcess.running) sourceProcess.running = true
    if (!preferencesLoaded && !doctorProcess.running) doctorProcess.running = true
  }

  function refreshStatus() {
    if (statusPath !== "") statusFile.reload()
    if (!statusProcess.running) statusProcess.running = true
  }

  function start(receiver, source, audio, quality, bitrate) {
    if (busy || !receiver || !source) return
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
    if (!doctorProcess.running) doctorProcess.running = true
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

  Component.onCompleted: refreshStatus()

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
        service.preferencesLoaded = true
        service.preferences = value.config.preferences || ({})
      })
    }
  }
}
