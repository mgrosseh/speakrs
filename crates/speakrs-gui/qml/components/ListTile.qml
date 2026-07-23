// UI Tile for displaying lists, like bookmarks, channel lists, user lists etc.

import com.kdab.cxx_qt.demo 1.0
import QtQuick

Rectangle {
    id: root
    property string title
    property ListModel list
    required property Component listItem
    anchors.fill: parent
    color: palette.alternateBase


    Text {
        id: titleTextObj
        anchors.top: parent.top
        anchors.horizontalCenter: parent.horizontalCenter
        color: palette.text
        text: root.title
    }

    ListView {
        id: listview
        anchors.top: titleTextObj.bottom
        anchors.topMargin: 10
        anchors.verticalCenter: parent.verticalCenter
        model: root.list

        delegate: root.listItem
        currentIndex: -1
        highlight: Rectangle { color: palette.highlight; radius: 5 }
        focus: true
    }
}
