use cxx_qt_build::{CxxQtBuilder, QmlModule};
fn main() {
    println!("cargo::rustc-env=QMAKE=/usr/bin/qmake6");
    let mut builder = CxxQtBuilder::new_qml_module(
        QmlModule::new("com.kdab.cxx_qt.demo")
            .qml_files(["qml/main/main.qml", "qml/components/ListTile.qml"]),
    )
    // Link Qt's Network library
    // - Qt Core is always linked
    // - Qt Gui is linked by enabling the qt_gui Cargo feature of cxx-qt-lib.
    // - Qt Qml is linked by enabling the qt_qml Cargo feature of cxx-qt-lib.
    // - Qt Qml requires linking Qt Network on macOS
    .qt_module("Network")
    .files(["src/cxxqt_object.rs"]);

    // Safety: "Misuse of the API may lead to unexpected behavior"... really cxxqt? This has no good reason to be marked unsafe.
    unsafe {
        // Silence benign warnings triggered from within Qt: https://gcc.gnu.org/bugzilla/show_bug.cgi?id=121244
        builder = builder.cc_builder(|cc| {
            cc.flag_if_supported("-Wsfinae-incomplete=0");
        });
    };

    builder.build();
}
