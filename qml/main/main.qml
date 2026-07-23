import QtQuick 2.12
import QtQuick.Controls 2.12
import QtQuick.Window 2.12

// This must match the uri and version
// specified in the qml_module in the build.rs script.
import com.kdab.cxx_qt.demo 1.0
import "../components"

ApplicationWindow {
    id: root
    height: 1920
    width: 1080
    title: qsTr("SpeakRS")
    visible: true
    color: palette.window
    // readonly property Username username: Username {
    //    string: qsTr("Change Username")
    //}

    Rectangle {
        id: titlebar
        width: parent.width
        height: 50
        color: palette.alternateBase

        Text {
            anchors.centerIn: parent
            color: palette.text
            text: "SpeakRS"
        }
    }

    MenuBar {
        Menu {
            title: qsTr("&Settings")
            Action { text: qsTr("&Profile") }
            Action { text: qsTr("&Audio") }
            Action { text: qsTr("&Video") }
            Action { text: qsTr("&Appearance") }
            Action { text: qsTr("&Accessibility") }
        }
        Menu {
            title: qsTr("&About and Help")
            Action { text: qsTr("&Github") }
            Action { text: qsTr("&Info") }
            Action { text: qsTr("&Debugging") }
        }
    }

    Item {
        anchors.top: titlebar.bottom
        anchors.topMargin: 10
        width: 200
        height: 200
        ListTile {
            id: bookmarkList
            title: qsTr("Server Bookmarks")
            list:  ListModel {
                ListElement {
                    index: 0
                    name: "Local Server"
                    address: "127.0.0.1"
                }
                ListElement {
                    index: 1
                    name: "Local Server 2"
                    address: "192.168.178.33"
                }
            }
            listItem: Component {
                id: savedServersDelegate
                Item {
                    id: serverEntry
                    required property int index
                    required property string name
                    required property string address
                    width: 180; height: 40
                    Column {
                        Text { text: '<b>Name:</b> ' + serverEntry.name; color: palette.text }
                        Text { text: '<b>Address:</b> ' + serverEntry.address; color: palette.text }
                    }
                    MouseArea {
                        id:mouseArea
                        anchors.fill: parent
                        hoverEnabled: true

                        onClicked: {
                            serverEntry.ListView.view.currentIndex = serverEntry.index
                            serverEntry.forceActiveFocus()
                        }
                    }
                }
            }
        }
    }

    // Component {
    //     id: savedServersDelegate
    //     Item {
    //         id: serverEntry
    //         required property int index
    //         required property string name
    //         required property string address
    //         width: 180; height: 40
    //         Column {
    //             Text { text: '<b>Name:</b> ' + serverEntry.name; color: palette.text }
    //             Text { text: '<b>Address:</b> ' + serverEntry.address; color: palette.text }
    //         }
    //         MouseArea {
    //             id:mouseArea
    //             anchors.fill: parent
    //             hoverEnabled: true

    //             onClicked: {
    //                 serverEntry.ListView.view.currentIndex = serverEntry.index
    //                 serverEntry.forceActiveFocus()
    //             }
    //         }
    //     }
    // }

//     Rectangle {
//         anchors.top: titlebar.bottom
//         anchors.topMargin: 10
//         width: 200
//         height: 200
//         color: palette.alternateBase

//         Text {
//             id: title
//             anchors.top: parent.top
//             anchors.horizontalCenter: parent.horizontalCenter
//             color: palette.text
//             text: qsTr("Server Bookmarks")
//         }

//         ListView {
//             id: listview
//             anchors.top: title.bottom
//             anchors.topMargin: 10
//             anchors.verticalCenter: parent.verticalCenter
//             model: ListModel {
//                 ListElement {
//                     index: 0
//                     name: "Local Server"
//                     address: "127.0.0.1"
//                 }
//                 ListElement {
//                     index: 1
//                     name: "Local Server 2"
//                     address: "192.168.178.33"
//                 }
//             }

//             delegate: savedServersDelegate
//             currentIndex: -1
//             highlight: Rectangle { color: palette.highlight; radius: 5 }
//             focus: true
//         }
//     }
}
