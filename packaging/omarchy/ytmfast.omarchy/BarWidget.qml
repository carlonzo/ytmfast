import QtQuick
import Quickshell.Services.Mpris
import qs.Commons
import qs.Ui

// ytmfast's tray icon as a bar plugin. The app serves MPRIS, the same
// interface its tray calls into, so this only reads and calls that bus. The
// icon shows only while ytmfast is running.
// Left click: controls, now playing, Open. Right click: Open, Quit.
BarWidget {
  id: root
  moduleName: "ytmfast.omarchy"

  readonly property var player: {
    var players = Mpris.players ? Mpris.players.values : []
    for (var i = 0; i < players.length; i++) {
      var p = players[i]
      if (p && (p.identity === "ytmfast" || p.desktopEntry === "ytmfast")) return p
    }
    return null
  }
  readonly property bool playing: player !== null && player.isPlaying
  readonly property string title: player && player.trackTitle ? player.trackTitle : "Nothing playing"
  readonly property string artist: player && player.trackArtist ? player.trackArtist : ""

  // "" (closed), "controls" (left click) or "menu" (right click). One card
  // serves both: popups sharing an owner share one coordinator.
  property string popup: ""

  function close() { popup = "" }
  function openApp() { if (player) player.raise(); close() }
  function quitApp() { if (player) player.quit(); close() }

  visible: player !== null
  implicitWidth: button.implicitWidth
  implicitHeight: button.implicitHeight

  // The app quit while a card was open: do not reopen it stale.
  onVisibleChanged: if (!visible) close()

  BarIconButton {
    id: button
    anchors.fill: parent
    bar: root.bar
    text: "󰝚"
    dimmed: !root.playing
    tooltipText: root.artist ? root.title + " — " + root.artist : root.title
    onPressed: function(b) {
      var next = b === Qt.RightButton ? "menu" : "controls"
      root.popup = root.popup === next ? "" : next
    }
  }

  PopupCard {
    id: card
    anchorItem: root
    bar: root.bar
    owner: root
    open: root.popup !== ""
    contentWidth: card.fittedContentWidth(Style.space(280))
    contentHeight: card.fittedContentHeight(root.popup === "menu" ? menuColumn.implicitHeight : controlsColumn.implicitHeight)

    Column {
      id: controlsColumn
      anchors.fill: parent
      spacing: Style.space(10)
      visible: root.popup === "controls"

      Text {
        width: parent.width
        textFormat: Text.PlainText
        text: root.title
        color: root.bar.foreground
        font.family: root.bar.fontFamily
        font.pixelSize: Style.font.subtitle
        font.bold: true
        elide: Text.ElideRight
      }

      Text {
        width: parent.width
        visible: root.artist !== ""
        textFormat: Text.PlainText
        text: root.artist
        color: Qt.darker(root.bar.foreground, 1.3)
        font.family: root.bar.fontFamily
        font.pixelSize: Style.font.bodySmall
        elide: Text.ElideRight
      }

      Row {
        anchors.horizontalCenter: parent.horizontalCenter
        spacing: Style.space(6)

        Button {
          iconText: "󰒮"
          foreground: root.bar.foreground
          horizontalPadding: Style.spacing.controlPaddingX
          verticalPadding: Style.spacing.controlPaddingY
          enabled: root.player !== null && root.player.canGoPrevious
          opacity: enabled ? 1.0 : 0.4
          onClicked: if (root.player) root.player.previous()
        }

        Button {
          iconText: root.player && root.player.isPlaying ? "󰏤" : "󰐊"
          foreground: root.bar.foreground
          horizontalPadding: Style.spacing.panelGap
          verticalPadding: Style.spacing.controlPaddingY
          iconSize: Style.font.iconLarge
          enabled: root.player !== null && root.player.canTogglePlaying
          opacity: enabled ? 1.0 : 0.4
          onClicked: if (root.player) root.player.togglePlaying()
        }

        Button {
          iconText: "󰒭"
          foreground: root.bar.foreground
          horizontalPadding: Style.spacing.controlPaddingX
          verticalPadding: Style.spacing.controlPaddingY
          enabled: root.player !== null && root.player.canGoNext
          opacity: enabled ? 1.0 : 0.4
          onClicked: if (root.player) root.player.next()
        }
      }

      Button {
        width: parent.width
        text: "Open"
        foreground: root.bar.foreground
        onClicked: root.openApp()
      }
    }

    Column {
      id: menuColumn
      anchors.fill: parent
      spacing: Style.space(4)
      visible: root.popup === "menu"

      Button {
        width: parent.width
        leftAlign: true
        text: "Open"
        foreground: root.bar.foreground
        onClicked: root.openApp()
      }

      Button {
        width: parent.width
        leftAlign: true
        text: "Quit"
        foreground: root.bar.foreground
        onClicked: root.quitApp()
      }
    }
  }
}
