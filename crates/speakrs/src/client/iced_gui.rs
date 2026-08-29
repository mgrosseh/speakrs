use crate::common::rpc::RpcServiceClient;
use crate::schema::ClientDataStore;

use super::ClientArguments;
use super::connection::Connection;
use iced::widget::{button, column, row, text, text_input};
use iced::{Element, Task, Theme};
use tracing::warn;

pub fn run(_args: ClientArguments) -> iced::Result {
    iced::application(Speakrs::new, Speakrs::update, Speakrs::view)
        .theme(Speakrs::theme)
        .run()
}

#[derive(Debug, Clone)]
enum Message {
    ChangeConnection(Connection),
    DisplayError(String),

    // Empty
    ConnectButtonPressed,
    IpFieldChanged(String),
}

#[allow(unused)] // TODO
enum State {
    Disconnected,
    Connected(Connected),
}

#[allow(unused)] // TODO
struct Connected {
    service: RpcServiceClient,
    store: ClientDataStore,
}

pub struct Speakrs {
    connection: Connection,

    // Empty:
    ip_input: String,
}

impl Speakrs {
    fn new() -> Self {
        Self {
            connection: Connection::Empty,
            ip_input: "".into(),
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::IpFieldChanged(value) => self.ip_input = value,
            Message::ChangeConnection(connection) => self.connection = connection,
            Message::DisplayError(error) => warn!(error),
            Message::ConnectButtonPressed => match &self.connection {
                Connection::Empty => {
                    return Task::perform(
                        Connection::connect_to_ip(self.ip_input.clone()),
                        |connection| match connection {
                            Ok(connection) => Message::ChangeConnection(connection),
                            Err(e) => Message::DisplayError(format!("{:?}", e)),
                        },
                    );
                }
                _ => {
                    warn!("Attempted to connect in non-Empty Connection state. Ignoring.");
                }
            },
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        match &self.connection {
            Connection::Empty => {
                let title = text("Speakrs");

                let connection_header = text("Connect to server IP");
                let ip_field = text_input("000.000.000.000:00000", &self.ip_input)
                    .on_input(Message::IpFieldChanged)
                    .padding(10)
                    .size(20);

                let connect_button = button("Connect").on_press(Message::ConnectButtonPressed);
                let connection_ui =
                    column![row![connection_header], row![ip_field, connect_button]];

                let ui = column![row![title], row![connection_ui]];

                ui.into()
            }
            Connection::Unregistered(_connection) => {
                todo!()
            }
            Connection::Active(_connection) => todo!(),
        }
    }

    fn theme(&self) -> Theme {
        Theme::Dracula
    }
}
