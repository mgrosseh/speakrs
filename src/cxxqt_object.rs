/// The bridge definition for our QObject
#[cxx_qt::bridge]
pub mod qobject {

    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        /// An alias to the QString type
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        // The QObject definition
        // We tell CXX-Qt that we want a QObject class with the name MyObject
        // based on the Rust struct MyObjectRust.
        #[qobject]
        #[qml_element]
        #[qproperty(QString, string)]
        #[namespace = "username"]
        type Username = super::UsernameRust;

        // Declare the invokable methods we want to expose on the QObject
        #[qinvokable]
        #[cxx_name = "checkUsername"]
        fn check_username(self: Pin<&mut Self>, string: &QString);
    }
}

use core::pin::Pin;
use cxx_qt_lib::QString;

/// The Rust struct for the QObject
#[derive(Default)]
pub struct UsernameRust {
    string: QString,
}

impl qobject::Username {
    /// TODO get stored username if existing and pass to ui
    pub fn check_username(self: Pin<&mut Self>, string: &QString) {
        self.set_string(string.clone());
    }
}