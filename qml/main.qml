import QtQuick 2.12
import QtQuick.Controls 2.12
import QtQuick.Window 2.12

// This must match the uri and version
// specified in the qml_module in the build.rs script.
import com.kdab.cxx_qt.demo 1.0

ApplicationWindow {
    id: root
    height: 1080
    title: qsTr("SpeakRS")
    visible: true
    width: 1920
    color: palette.window

    readonly property Username username: Username {
        string: qsTr("Change Username")
    }

    Column {
        anchors.fill: parent
        anchors.margins: 10
        spacing: 10

        Row {
            spacing: 100
            Item {
                width: 250; height: 50

                Rectangle {
                    color: "purple"
                    width: 250
                    height: 50
                }
                TextInput { id: changeUsername; text: root.username.string }

            }
            Item {
                width: 300; height: 50

                Rectangle {
                    anchors.fill: parent
                    color: "black"
                }
                Label {
                    anchors.centerIn: parent
                    text: qsTr("User: %1").arg(changeUsername.text)
                    font.pointSize: 20
                    color: palette.text
                }
            }
        }

    }
}
