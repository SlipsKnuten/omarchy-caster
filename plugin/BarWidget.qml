import QtQuick
import QtQuick.Controls as QQC
import Quickshell
import qs.Commons
import qs.Ui
import "Model.js" as Model

Panel {
  id: root

  moduleName: "slipsknuten.caster"
  ipcTarget: "slipsknuten.caster"

  property string selectedReceiverId: ""
  property string selectedSourceId: ""
  property string sourceKind: "window"
  property bool audioEnabled: false
  property bool advancedOpen: false
  property string quality: "auto"
  property string bitrate: "auto"
  readonly property string installGuideUrl: "https://github.com/SlipsKnuten/omarchy-caster#install"

  readonly property bool active: Model.isActive(caster.session.state)
  readonly property var visibleSources: Model.byKind(caster.sources, sourceKind)
  readonly property var selectedReceiver: Model.findById(caster.receivers, selectedReceiverId)
  readonly property var selectedSource: Model.findById(caster.sources, selectedSourceId)
  readonly property color foreground: bar ? bar.foreground : Color.popups.text
  readonly property color urgent: bar ? bar.urgent : Color.urgent
  readonly property color dim: Qt.darker(foreground, 1.5)
  readonly property string fontFamily: bar ? bar.fontFamily : Style.font.family

  function synchronizeSelections() {
    selectedReceiverId = Model.firstId(caster.receivers, selectedReceiverId)
    var available = Model.byKind(caster.sources, sourceKind)
    selectedSourceId = Model.firstId(available, selectedSourceId)
    if (available.length === 0) {
      var alternative = sourceKind === "window" ? "screen" : "window"
      var other = Model.byKind(caster.sources, alternative)
      if (other.length > 0) {
        sourceKind = alternative
        selectedSourceId = Model.firstId(other, "")
      }
    }
  }

  function selectKind(kind) {
    sourceKind = kind
    selectedSourceId = Model.firstId(Model.byKind(caster.sources, kind), "")
    audioEnabled = false
  }

  function beginCast() {
    var audio = audioEnabled ? (sourceKind === "window" ? "application" : "system") : "off"
    caster.start(selectedReceiver, selectedSource, audio, quality, bitrate)
  }

  function applyPreferences() {
    var preferences = caster.preferences || ({})
    if (preferences.receiver_id) selectedReceiverId = preferences.receiver_id
    if (preferences.source_kind === "window" || preferences.source_kind === "screen")
      sourceKind = preferences.source_kind
    audioEnabled = preferences.audio !== undefined && preferences.audio !== "off"
    if (preferences.quality) quality = preferences.quality
    if (preferences.bitrate) bitrate = preferences.bitrate
    synchronizeSelections()
  }

  implicitWidth: barButton.implicitWidth
  implicitHeight: barButton.implicitHeight

  onOpenedChanged: if (opened) {
    caster.refresh()
    caster.refreshStatus()
  }

  Service { id: caster }

  Connections {
    target: caster
    function onReceiversChanged() { root.synchronizeSelections() }
    function onSourcesChanged() { root.synchronizeSelections() }
    function onPreferencesChanged() { root.applyPreferences() }
  }

  BarIconButton {
    id: barButton
    anchors.fill: parent
    bar: root.bar
    text: "󰄘"
    active: root.active
    tooltipText: root.active ? "Casting to " + (caster.session.device || "receiver") : "Caster"
    onPressed: function(button) {
      if (button === Qt.RightButton) caster.refresh()
      else root.toggle()
    }
  }

  Shortcut {
    sequence: "Escape"
    enabled: root.opened
    onActivated: root.close()
  }

  KeyboardPanel {
    id: panel
    anchorItem: barButton
    owner: root
    bar: root.bar
    open: root.opened
    focusTarget: root.active ? stopButton : (caster.ready ? castButton : guideButton)
    contentWidth: panel.fittedContentWidth(Style.space(420))
    contentHeight: panel.fittedContentHeight(content.implicitHeight, Style.space(680))

    Flickable {
      id: scroller
      anchors.fill: parent
      contentWidth: width
      contentHeight: content.implicitHeight
      clip: true
      boundsBehavior: Flickable.StopAtBounds
      flickableDirection: Flickable.VerticalFlick
      interactive: contentHeight > height
      QQC.ScrollBar.vertical: QQC.ScrollBar { policy: QQC.ScrollBar.AsNeeded }

      Column {
        id: content
        width: scroller.width
        spacing: Style.space(12)

        PanelHero {
          width: parent.width
          title: root.active ? (caster.session.device || "Caster") : "Caster"
          meta: Model.stageLabel(caster.session.state)
          detail: root.active ? (caster.session.resolved_quality || "LIVE") : "CAST"
          foreground: root.foreground
          fontFamily: root.fontFamily
          iconComponent: Component {
            Text {
              text: root.active ? "󰄙" : "󰄘"
              textFormat: Text.PlainText
              color: root.active ? Color.accent : root.foreground
              font.family: root.fontFamily
              font.pixelSize: Style.font.display
            }
          }
        }

        Column {
          visible: caster.installChecked && !caster.ready
          width: parent.width
          spacing: Style.space(10)

          PanelSectionHeader {
            text: "SETUP REQUIRED"
            foreground: root.foreground
            fontFamily: root.fontFamily
          }

          Text {
            width: parent.width
            text: "Caster is installed as a plugin, but it needs its helper programs before it can cast."
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            color: root.foreground
            font.family: root.fontFamily
            font.pixelSize: Style.font.body
          }

          Repeater {
            model: caster.setupIssues
            Text {
              required property var modelData
              width: parent.width
              text: "\u2022 " + modelData
              textFormat: Text.PlainText
              wrapMode: Text.WordWrap
              color: root.urgent
              font.family: root.fontFamily
              font.pixelSize: Style.font.body
            }
          }

          Text {
            width: parent.width
            text: "Follow the install steps, then check again."
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            color: root.dim
            font.family: root.fontFamily
            font.pixelSize: Style.font.bodySmall
          }

          Row {
            width: parent.width
            spacing: Style.spacing.controlGap

            Button {
              id: guideButton
              width: (parent.width - parent.spacing) / 2
              text: "Install guide"
              bordered: true
              focusable: true
              onClicked: Quickshell.execDetached(["xdg-open", root.installGuideUrl])
            }
            Button {
              width: (parent.width - parent.spacing) / 2
              text: caster.scanning ? "Checking\u2026" : "Check again"
              bordered: true
              focusable: true
              onClicked: caster.refresh()
            }
          }
        }

        Text {
          visible: caster.error !== "" || caster.session.state === "error"
          width: parent.width
          text: caster.error || caster.session.detail
          textFormat: Text.PlainText
          wrapMode: Text.WordWrap
          color: root.urgent
          font.family: root.fontFamily
          font.pixelSize: Style.font.body
        }

        Column {
          visible: root.active
          width: parent.width
          spacing: Style.space(10)

          PanelSectionHeader {
            text: "NOW CASTING"
            foreground: root.foreground
            fontFamily: root.fontFamily
          }

          CastChoiceRow {
            width: parent.width
            title: caster.session.source || "Preparing source…"
            subtitle: caster.session.audio_detail || "Audio off"
            leadingIcon: caster.session.source_kind === "window" ? "󰖲" : "󰍹"
            selected: true
            busy: true
          }

          Text {
            width: parent.width
            text: caster.session.detail || "Streaming"
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            color: root.dim
            font.family: root.fontFamily
            font.pixelSize: Style.font.bodySmall
          }

          Button {
            id: stopButton
            width: parent.width
            text: caster.session.state === "stopping" ? "Stopping…" : "Stop casting"
            iconText: "󰓛"
            foreground: root.urgent
            bordered: true
            focusable: true
            enabled: !caster.busy && caster.session.state !== "stopping"
            onClicked: caster.stop()
          }
        }

        Column {
          visible: !root.active && caster.ready
          width: parent.width
          spacing: Style.space(12)

          PanelSectionHeader {
            text: "CHOOSE A RECEIVER"
            foreground: root.foreground
            fontFamily: root.fontFamily
          }

          Text {
            visible: caster.receivers.length === 0
            width: parent.width
            text: caster.scanning ? "Looking for Google Cast devices…" : "No receivers found on this network."
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            horizontalAlignment: Text.AlignHCenter
            color: root.dim
            font.family: root.fontFamily
            font.pixelSize: Style.font.body
          }

          Repeater {
            model: caster.receivers
            CastChoiceRow {
              required property var modelData
              width: parent.width
              title: modelData.name
              subtitle: modelData.model
              leadingIcon: "󰒋"
              selected: root.selectedReceiverId === modelData.id
              busy: caster.busy
              onChosen: root.selectedReceiverId = modelData.id
            }
          }

          Row {
            width: parent.width
            spacing: Style.spacing.controlGap

            Button {
              width: (parent.width - parent.spacing) / 2
              text: "Applications"
              iconText: "󰖲"
              selected: root.sourceKind === "window"
              bordered: true
              focusable: true
              onClicked: root.selectKind("window")
            }
            Button {
              width: (parent.width - parent.spacing) / 2
              text: "Screens"
              iconText: "󰍹"
              selected: root.sourceKind === "screen"
              bordered: true
              focusable: true
              onClicked: root.selectKind("screen")
            }
          }

          Text {
            width: parent.width
            text: Model.sourceDescription(root.sourceKind)
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            color: root.dim
            font.family: root.fontFamily
            font.pixelSize: Style.font.bodySmall
          }

          Text {
            visible: root.visibleSources.length === 0
            width: parent.width
            text: caster.scanning ? "Reading available sources…" : "No shareable " + (root.sourceKind === "window" ? "applications" : "screens") + " found."
            textFormat: Text.PlainText
            horizontalAlignment: Text.AlignHCenter
            color: root.dim
            font.family: root.fontFamily
            font.pixelSize: Style.font.body
          }

          Repeater {
            model: root.visibleSources
            CastChoiceRow {
              required property var modelData
              width: parent.width
              title: modelData.label
              subtitle: root.sourceKind === "window" ? modelData.app : "Entire display"
              leadingIcon: root.sourceKind === "window" ? "󰖲" : "󰍹"
              selected: root.selectedSourceId === modelData.id
              busy: caster.busy
              onChosen: root.selectedSourceId = modelData.id
            }
          }

          Toggle {
            width: parent.width
            label: root.sourceKind === "window" ? "Application audio" : "System audio"
            description: root.sourceKind === "window"
              ? "Send matching app audio to the TV; other apps stay local."
              : "Send the desktop mix to the TV."
            checked: root.audioEnabled
            enabled: !caster.busy && root.selectedSource !== null
            onClicked: root.audioEnabled = !root.audioEnabled
          }

          Button {
            width: parent.width
            text: root.advancedOpen ? "Hide advanced settings" : "Advanced settings"
            iconText: root.advancedOpen ? "󰅃" : "󰅀"
            leftAlign: true
            focusable: true
            onClicked: root.advancedOpen = !root.advancedOpen
          }

          Column {
            visible: root.advancedOpen
            width: parent.width
            spacing: Style.space(10)

            Dropdown {
              width: parent.width
              label: "Quality"
              value: root.quality
              options: Model.qualityOptions()
              foreground: root.foreground
              onChanged: function(value) { root.quality = value }
            }
            Dropdown {
              width: parent.width
              label: "Video bitrate"
              value: root.bitrate
              options: Model.bitrateOptions()
              foreground: root.foreground
              onChanged: function(value) { root.bitrate = value }
            }
          }

          PanelSeparator {
            foreground: root.foreground
          }

          Button {
            id: castButton
            width: parent.width
            text: caster.busy ? "Starting…" : "Cast now"
            iconText: "󰄘"
            active: root.selectedReceiver !== null && root.selectedSource !== null
            bordered: true
            focusable: true
            enabled: !caster.busy && root.selectedReceiver !== null && root.selectedSource !== null
            onClicked: root.beginCast()
          }

          Row {
            width: parent.width
            spacing: Style.spacing.controlGap
            Button {
              width: (parent.width - parent.spacing) / 2
              text: caster.scanning ? "Scanning…" : "Refresh"
              iconText: "󰑐"
              focusable: true
              enabled: !caster.scanning && !caster.busy
              onClicked: caster.refresh()
            }
            Button {
              width: (parent.width - parent.spacing) / 2
              text: "Diagnostics"
              iconText: "󰒓"
              focusable: true
              enabled: !caster.busy
              onClicked: caster.runDoctor()
            }
          }

          Text {
            visible: caster.diagnostics !== ""
            width: parent.width
            text: caster.diagnostics
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            color: root.dim
            font.family: root.fontFamily
            font.pixelSize: Style.font.bodySmall
          }
        }
      }
    }
  }
}
