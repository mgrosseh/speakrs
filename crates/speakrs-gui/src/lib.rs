use gtk::gio::ApplicationFlags;
use gtk::prelude::*;
use gtk::{Application, ApplicationWindow, glib};
use eyre::Result;

const APP_ID: &str = "speakrs.GtkGui";

pub fn run() -> Result<()> {
    let app = Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    let application = app.run_with_args::<&str>(&[]);
    if application == glib::ExitCode::FAILURE {
        return Ok(()) // TODO return error
    }
    tracing::info!("info");
    tracing::info!("{:#?}", application);
    return Ok(())
}

fn build_ui(app: &Application) {
    let window = ApplicationWindow::builder()
        .application(app)
        .title("SpeakRS")
        .build();

    window.present();
}
