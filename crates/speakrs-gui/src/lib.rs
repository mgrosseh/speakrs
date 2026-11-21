use gtk::gdk::Display;
use gtk::prelude::*;
use gtk::{Align, Button, CssProvider, Label, Orientation};
use gtk::{Application, ApplicationWindow, glib};
use eyre::Result;

const APP_ID: &str = "speakrs.GtkGui";

pub fn run() -> Result<()> {
    let app = Application::builder().application_id(APP_ID).build();
    app.connect_startup(|_| load_css());
    app.connect_activate(build_ui);
    let application = app.run_with_args::<&str>(&[]);
    if application == glib::ExitCode::FAILURE {
        return Ok(()) // TODO return error
    }
    tracing::info!("info");
    tracing::info!("{:#?}", application);
    return Ok(())
}

fn load_css() {
    let provider = CssProvider::new();
    provider.load_from_string(include_str!("../css/style.css"));

    gtk::style_context_add_provider_for_display(
        &Display::default().expect("Could not connect to a display."),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

fn build_ui(app: &Application) {
    let title_label = Label::builder()
        .label("SpeakRS")
        .margin_top(3)
        .halign(Align::Center)
        .build();

    let disconnect_button = Button::builder()
        .label("Disconnect")
        .margin_top(3)
        .margin_start(12)
        .build();

    let settings_button = Button::builder()
        .label("Options")
        .margin_top(3)
        .build();

    let info_button = Button::builder()
        .label("Info")
        .margin_top(3)
        .margin_end(12)
        .build();

    let main_box = gtk::Box::builder()
        .orientation(Orientation::Vertical)
        .valign(Align::Start)
        .hexpand(true)
        .vexpand(true)
        .build();

    let title_bar_box = gtk::Box::builder()
        .orientation(Orientation::Horizontal)
        .homogeneous(true)
        .valign(Align::Start)
        .halign(Align::Fill)
        .build();

    let title_bar_left = gtk::Box::builder()
        .orientation(Orientation::Horizontal)
        .homogeneous(false)
        .halign(Align::Start)
        .build();

    let title_bar_right = gtk::Box::builder()
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

    let settings_box = gtk::ListBox::builder()
        .build();

    main_box.append(&title_bar_box);
    main_box.append(&server_utils_box);
    title_bar_box.add_css_class("titlebar-1");
    title_bar_box.append(&title_bar_left);
    title_bar_box.append(&title_label);
    title_bar_box.append(&title_bar_right);
    title_bar_left.append(&disconnect_button);
    title_bar_right.append(&settings_button);
    title_bar_right.append(&info_button);
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

    let settings_box_label = gtk::Label::builder()
        .label("Settings")
        .selectable(false)
        .build();

    settings_box.append(&settings_box_label);

    let settings_window = ApplicationWindow::builder()
        .application(app)
        .title("SpeakRS Settings")
        .child(&settings_box)
        .build();

    settings_button.connect_clicked(move |_| settings_window.present());
}
