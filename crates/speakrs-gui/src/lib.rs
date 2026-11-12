use gtk::{Align, Button, Label, Orientation, prelude::*};
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
    let settings_button = Button::builder()
        .label("Options")
        .margin_top(3)
        .margin_start(12)
        .valign(Align::Start)
        .build();

    let info_button = Button::builder()
        .label("Info")
        .margin_top(3)
        .margin_end(12)
        .valign(Align::Start)
        .build();

    let main_box = gtk::Box::builder()
        .orientation(Orientation::Vertical)
        .valign(Align::Start)
        .build();

    let title_bar_box = gtk::Box::builder()
        .orientation(Orientation::Horizontal)
        .homogeneous(false)
        .halign(Align::End)
        .build();

    let server_utils_box = gtk::Box::builder()
        .orientation(Orientation::Horizontal)
        .margin_top(12)
        .build();

    let server_list_box = gtk::ListBox::builder()
        .halign(Align::Start)
        .hexpand(true)
        .margin_start(12)
        .margin_end(12)
        .build();

    let server_channel_list_box = gtk::ListBox::builder()
        .halign(Align::Center)
        .hexpand(true)
        .margin_start(12)
        .margin_end(12)
        .build();

    let server_user_list_box = gtk::ListBox::builder()
        .halign(Align::End)
        .hexpand(true)
        .margin_start(12)
        .margin_end(12)
        .build();

    main_box.append(&title_bar_box);
    title_bar_box.append(&settings_button);
    title_bar_box.append(&info_button);
    main_box.append(&server_utils_box);
    server_utils_box.append(&server_list_box);
    server_utils_box.append(&server_channel_list_box);
    server_utils_box.append(&server_user_list_box);

    let server_list_label = gtk::Label::builder()
        .label("Server List")
        .selectable(false)
        .build();
    let server_local_button = gtk::CheckButton::builder()
        .label("localhost")
        .build();
    server_list_box.append(&server_list_label);
    server_list_box.append(&server_local_button);

    let server_channel_list_label = gtk::Label::builder()
        .label("Channel List")
        .selectable(false)
        .build();
    server_channel_list_box.append(&server_channel_list_label);

    let server_user_list_label = gtk::Label::builder()
        .label("User")
        .selectable(false)
        .build();
    server_user_list_box.append(&server_user_list_label);

    let window = ApplicationWindow::builder()
        .application(app)
        .title("SpeakRS")
        .child(&main_box)
        .build();

    window.present();
}
