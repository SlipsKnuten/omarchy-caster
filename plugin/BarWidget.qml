import QtQuick
import QtQuick.Controls
import Quickshell
import Quickshell.Io
import qs.Ui
import qs.Commons

BarWidget {
  id: root
  moduleName: "pepw.native-cast-dev"
  implicitWidth: 36
  implicitHeight: barSize
  property bool opened: false
  property var receivers: []
  property var sources: []
  property var session: ({state: "idle", detail: "Ready"})
  property string error: ""
  readonly property string executable: Quickshell.env("HOME") + "/.local/bin/omacast-dev"
  readonly property bool active: session.state === "streaming" || session.state === "connecting"
  readonly property color fg: bar ? bar.foreground : "#eeeeee"
  readonly property var selectedSource: sources[sourcePicker.currentIndex] || null
  readonly property bool windowMode: root.active ? root.session.source_kind === "window" : selectedSource !== null && selectedSource.kind === "window"
  function close() { opened = false }
  function open() { opened = true }
  function toggle() { opened = !opened }
  function decode(text) {
    try {
      var value = JSON.parse(text)
      if (value.api !== 1) throw new Error("Controller API mismatch")
      if (!value.ok) throw new Error(value.error || "Command failed")
      return value.result
    } catch (e) { root.error = String(e); return null }
  }
  function refresh() {
    if (!deviceProcess.running) deviceProcess.running = true
    if (!sourceProcess.running) sourceProcess.running = true
  }
  function action(args) {
    if (actionProcess.running) return
    root.error = ""
    actionProcess.command = [root.executable].concat(args)
    actionProcess.running = true
  }
  onOpenedChanged: if (opened && !active) refresh()

  Text {
    anchors.centerIn: parent
    text: "󰄘"
    textFormat: Text.PlainText
    color: root.active ? Color.accent : root.fg
    font.family: root.bar ? root.bar.fontFamily : "monospace"
    font.pixelSize: 20
  }
  MouseArea {
    anchors.fill: parent
    cursorShape: Qt.PointingHandCursor
    onClicked: root.toggle()
  }
  PopupCard {
    id: popup
    anchorItem: root
    bar: root.bar
    owner: root
    open: root.opened
    contentWidth: popup.fittedContentWidth(440)
    contentHeight: popup.fittedContentHeight(content.implicitHeight)
    Column {
      id: content
      width: parent.width
      spacing: 12
      Text { text: "Native Cast · Development"; textFormat: Text.PlainText; color: root.fg; font.bold: true; font.pixelSize: 18 }
      Text { text: "Receiver"; color: root.fg }
      ComboBox {
        id: receiverPicker
        visible: !root.active
        width: parent.width
        model: root.receivers
        textRole: "name"
        enabled: !root.active && !actionProcess.running
        contentItem: Text { text: receiverPicker.displayText; textFormat: Text.PlainText; elide: Text.ElideRight; color: root.fg; verticalAlignment: Text.AlignVCenter; leftPadding: 8 }
      }
      Text { visible: root.active; width: parent.width; text: root.session.device || "Connecting…"; textFormat: Text.PlainText; wrapMode: Text.WordWrap; color: root.fg }
      Text { text: "Screen or application window"; color: root.fg }
      ComboBox {
        id: sourcePicker
        visible: !root.active
        width: parent.width
        model: root.sources
        textRole: "label"
        enabled: !root.active && !actionProcess.running
        contentItem: Text { text: sourcePicker.displayText; textFormat: Text.PlainText; elide: Text.ElideRight; color: root.fg; verticalAlignment: Text.AlignVCenter; leftPadding: 8 }
        onCurrentIndexChanged: audioCheck.checked = root.windowMode
      }
      Text { visible: root.active; width: parent.width; text: root.session.source || "Preparing source…"; textFormat: Text.PlainText; wrapMode: Text.WordWrap; color: root.fg }
      CheckBox {
        id: audioCheck
        visible: !root.active
        text: root.windowMode ? "Application audio → TV only" : "Share all system audio"
        enabled: !root.active && !actionProcess.running
      }
      Text {
        width: parent.width
        text: root.windowMode ? "Only the selected window is captured. Audio is application-wide: browser tabs may share audio. Unmatched streams stay local." : "The entire selected screen is shared, including notifications. System audio is optional and may also play locally."
        textFormat: Text.PlainText
        wrapMode: Text.WordWrap
        color: root.fg
        opacity: 0.75
        font.pixelSize: 12
      }
      Text { width: parent.width; text: "1080p · 30 fps · Original proportions"; color: root.fg; font.pixelSize: 12 }
      Row {
        spacing: 10
        Button {
          text: root.active ? "Stop casting" : "Start casting"
          enabled: !actionProcess.running && (root.active || (receiverPicker.currentIndex >= 0 && sourcePicker.currentIndex >= 0))
          onClicked: {
            if (root.active) root.action(["stop"])
            else root.action(["start", "--device", root.receivers[receiverPicker.currentIndex].id,
                              "--source", root.selectedSource.id, "--audio",
                              audioCheck.checked ? (root.windowMode ? "application" : "system") : "off"])
          }
        }
        Button { text: deviceProcess.running ? "Scanning…" : "Refresh"; enabled: !root.active && !deviceProcess.running; onClicked: root.refresh() }
      }
      Text {
        width: parent.width
        text: root.error || ((root.session.state || "idle") + " · " + (root.session.detail || ""))
        textFormat: Text.PlainText
        wrapMode: Text.WordWrap
        color: root.error || root.session.state === "error" ? "#ff8888" : root.fg
      }
      Text {
        width: parent.width
        visible: root.active && !!root.session.audio_detail
        text: root.session.audio_detail || ""
        textFormat: Text.PlainText
        wrapMode: Text.WordWrap
        color: root.fg
      }
    }
  }
  Timer { interval: 1000; running: true; repeat: true; triggeredOnStart: true; onTriggered: if (!statusProcess.running) statusProcess.running = true }
  Process {
    id: statusProcess
    command: [root.executable, "status"]
    stdout: StdioCollector { onStreamFinished: { var value = root.decode(text); if (value) root.session = value } }
  }
  Process {
    id: deviceProcess
    command: [root.executable, "devices"]
    stdout: StdioCollector { onStreamFinished: { var value = root.decode(text); if (value) root.receivers = value } }
  }
  Process {
    id: sourceProcess
    command: [root.executable, "sources"]
    stdout: StdioCollector { onStreamFinished: { var value = root.decode(text); if (value) root.sources = value } }
  }
  Process {
    id: actionProcess
    stdout: StdioCollector { onStreamFinished: { var value = root.decode(text); if (value) root.session = value } }
  }
}
