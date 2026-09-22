import QtQuick
import qs.Commons
import qs.Ui

CursorSurface {
  id: root

  required property string title
  property string subtitle: ""
  property string leadingIcon: ""
  property bool selected: false
  property bool busy: false

  signal chosen()

  current: selected
  bordered: true
  foreground: Color.popups.text
  implicitHeight: Math.max(Style.space(48), labels.implicitHeight + Style.spacing.rowPaddingX * 2)

  Row {
    anchors.left: parent.left
    anchors.right: parent.right
    anchors.verticalCenter: parent.verticalCenter
    anchors.leftMargin: root.borderLeft + Style.spacing.rowPaddingX
    anchors.rightMargin: root.borderRight + Style.spacing.rowPaddingX
    spacing: Style.spacing.controlGap

    Text {
      width: Style.space(24)
      anchors.verticalCenter: parent.verticalCenter
      horizontalAlignment: Text.AlignHCenter
      text: root.leadingIcon
      textFormat: Text.PlainText
      color: root.selected ? Color.accent : root.foreground
      font.family: Style.font.family
      font.pixelSize: Style.font.icon
    }

    Column {
      id: labels
      width: parent.width - Style.space(24) - check.width - parent.spacing * 2
      spacing: Style.spacing.xxs
      anchors.verticalCenter: parent.verticalCenter

      Text {
        width: parent.width
        text: root.title
        textFormat: Text.PlainText
        color: root.foreground
        font.family: Style.font.family
        font.pixelSize: Style.font.body
        font.bold: root.selected
        elide: Text.ElideRight
      }
      Text {
        visible: root.subtitle !== ""
        width: parent.width
        text: root.subtitle
        textFormat: Text.PlainText
        color: Qt.darker(root.foreground, 1.45)
        font.family: Style.font.family
        font.pixelSize: Style.font.caption
        elide: Text.ElideRight
      }
    }

    Text {
      id: check
      width: Style.space(18)
      anchors.verticalCenter: parent.verticalCenter
      text: root.busy ? "󰑓" : (root.selected ? "󰄬" : "")
      textFormat: Text.PlainText
      color: root.selected ? Color.accent : root.foreground
      font.family: Style.font.family
      font.pixelSize: Style.font.body
    }
  }

  MouseArea {
    anchors.fill: parent
    enabled: !root.busy
    hoverEnabled: true
    cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
    onEntered: root.hasCursor = true
    onExited: root.hasCursor = false
    onClicked: root.chosen()
  }
}
