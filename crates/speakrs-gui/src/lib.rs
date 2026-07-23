use cxx_qt::casting::Upcast;
use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QQmlEngine, QUrl, QString};
use core::pin::Pin;

mod cxxqt_object;

pub fn run() {
    // Create the application and engine
    let mut app = QGuiApplication::new();
    let mut engine = QQmlApplicationEngine::new();

    // Load the QML path into the engine
    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from(r#"qrc:/qt/qml/com/kdab/cxx_qt/demo/qml/main/main.qml"#));
    }

    if let Some(engine) = engine.as_mut() {
        engine.add_import_path(&QString::from("qrc:/qt/qml/com/kdab/cxx_qt/demo/qml/components"));
    }

    if let Some(engine) = engine.as_mut() {
        let engine: Pin<&mut QQmlEngine> = engine.upcast_pin();
        // Listen to a signal from the QML Engine
        engine
            .on_quit(|_| {
                println!("QML Quit!");
            })
            .release();
    }

    // Start the app
    if let Some(app) = app.as_mut() {
        app.exec();
    }
}
