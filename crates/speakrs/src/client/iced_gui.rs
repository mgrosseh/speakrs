use std::fmt::Display;
use std::sync::Arc;

use crate::common::database::open_client_db;
use crate::common::rpc::RpcServiceClient;
use crate::schema::{ClientDataStore, client::client_session::ClientSession, user::User};

use super::ClientArguments;
use eyre::{Result, ResultExt};
use iced::widget::{Column, button, column, pick_list, row, scrollable, text, text_input};
use iced::{Element, Task, Theme};
use speakrs_storage::pagination::{Edge, Pagination};
use tarpc::tokio_serde::formats::Json;
use tokio::net::ToSocketAddrs;
use tracing::{Instrument, info_span, warn};

pub fn run(_args: ClientArguments) -> iced::Result {
    iced::application(
        move || (Speakrs::new(), startup()),
        Speakrs::update,
        Speakrs::view,
    )
    .theme(Speakrs::theme)
    .run()
}

fn startup() -> Task<Message> {
    // Ex.  using multiple tasks doing startup for a specific window, e.g. use iced::daemon
    //let mut startup_task_chain = window::latest().map(Message::StartApp);
    // startup_task_chain = startup_task_chain
    // // do things
    //     .chain(Task::done(Message::Start));
    //startup_task_chain
    Task::done(Message::Start)
}

enum State {
    Disconnected,
    Connected(ConnectionHandle),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RegisterLoginChoice {
    Login,
    Register,
}
impl Display for RegisterLoginChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Login => "Login",
            Self::Register => "Register",
        })
    }
}

#[derive(Debug, Clone)]
enum Message {
    Start,

    Connect(ConnectionHandle),
    LoginStatusChanged(bool),
    DisplayError(String),

    // Disconnected
    ConnectButtonPressed,
    IpFieldChanged(String),
    // Connected | !logged_in
    RegisterLoginChoiceSelected(RegisterLoginChoice),
    LoginUsernameInputChanged(String),
    LoginPasswordInputChanged(String),
    LoginRegisterButtonPressed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoginStatus {
    Unknown,
    LoggedIn,
    LoggedOut,
}

pub struct Speakrs {
    state: State,
    login_status: LoginStatus,

    // Disconnected:
    ip_input: String,
    // Connected | !logged_in
    register_login_choice: Option<RegisterLoginChoice>,
    login_username_input: String,
    login_password_input: String,
}

impl Speakrs {
    fn new() -> Self {
        Self {
            state: State::Disconnected,
            login_status: LoginStatus::Unknown,
            ip_input: "127.0.0.1:51777".into(),
            register_login_choice: None,
            login_username_input: "".into(),
            login_password_input: "".into(),
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Start => return self.start(),
            Message::IpFieldChanged(value) => self.ip_input = value,
            Message::Connect(connection) => {
                self.state = State::Connected(connection.clone());
                return Task::perform(connection.check_login(), |value| match value {
                    Ok(v) => Message::LoginStatusChanged(v),
                    Err(e) => Message::DisplayError(e.to_string()),
                });
            }
            Message::LoginStatusChanged(value) => match value {
                true => self.login_status = LoginStatus::LoggedIn,
                false => self.login_status = LoginStatus::LoggedOut,
            },
            Message::DisplayError(error) => warn!(error),
            // Disconnected
            Message::ConnectButtonPressed => match &self.state {
                State::Disconnected => {
                    return Task::perform(
                        ConnectionController::connect_to_ip(self.ip_input.clone()),
                        |connection| match connection {
                            Ok(connection) => Message::Connect(connection),
                            Err(e) => Message::DisplayError(format!("{:?}", e)),
                        },
                    );
                }
                State::Connected(_) => warn!("Attempted to connect in Connected state. Ignoring."),
            },
            // Connected | !logged_in
            Message::RegisterLoginChoiceSelected(choice) => {
                self.register_login_choice = Some(choice)
            }
            Message::LoginPasswordInputChanged(password) => self.login_password_input = password,
            Message::LoginUsernameInputChanged(username) => self.login_username_input = username,
            Message::LoginRegisterButtonPressed => {
                match &self.state {
                    State::Connected(connection) => {
                        match self.register_login_choice {
                            Some(RegisterLoginChoice::Login) => {
                                return Task::perform(
                                    connection.clone().login(
                                        self.login_username_input.clone(),
                                        self.login_password_input.clone(),
                                    ),
                                    |result| match result {
                                        Ok(_) => Message::LoginStatusChanged(true),
                                        Err(e) => Message::DisplayError(e.to_string()),
                                    },
                                );
                            }
                            Some(RegisterLoginChoice::Register) => {
                                return Task::perform(
                                    connection.clone().register_login(
                                        self.login_username_input.clone(),
                                        self.login_password_input.clone(),
                                    ),
                                    |result| match result {
                                        Ok(_) => Message::LoginStatusChanged(true),
                                        Err(e) => Message::DisplayError(e.to_string()),
                                    },
                                );
                            }
                            None => warn!(
                                "Clicked login/register button without selecting register or login. Ignoring."
                            ), // TODO tell user this
                        }
                    }
                    _ => warn!("Attempted to login in Disconnected state. Ignoring."),
                }
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        match &self.state {
            State::Disconnected => {
                let title = text("Speakrs");

                let connection_header = text("Connect to server IP");
                let ip_field = text_input("000.000.000.000:00000", &self.ip_input)
                    .on_input(Message::IpFieldChanged)
                    .padding(10);

                let connect_button = button("Connect").on_press(Message::ConnectButtonPressed);
                let connection_ui =
                    column![row![connection_header], row![ip_field, connect_button]];

                let ui = column![row![title], row![connection_ui]];

                ui.into()
            }
            State::Connected(connection) => {
                let title = text("Speakrs");
                match self.login_status {
                    LoginStatus::LoggedOut => {
                        let info = text("No session was found, you need to log in or register.");

                        let header = text("Log in or Register");

                        let username_label = text("Username:");
                        let username_field = text_input("", &self.login_username_input)
                            .on_input(Message::LoginUsernameInputChanged)
                            .padding(10);
                        let password_label = text("Password:");
                        let password_field = text_input("", &self.login_password_input)
                            .on_input(Message::LoginPasswordInputChanged)
                            .padding(10);
                        let register_login_choice = pick_list(
                            [RegisterLoginChoice::Login, RegisterLoginChoice::Register],
                            self.register_login_choice,
                            Message::RegisterLoginChoiceSelected,
                        );
                        let login_button =
                            button("Connect").on_press(Message::LoginRegisterButtonPressed);

                        let ui = column![
                            title,
                            info,
                            column![
                                header,
                                row![username_label, username_field],
                                row![password_label, password_field],
                                row![register_login_choice, login_button]
                            ]
                        ];

                        ui.into()
                    }
                    LoginStatus::Unknown => {
                        text("Error. Unknown login state after trying to connect, this is a bug.")
                            .into()
                    }
                    LoginStatus::LoggedIn => {
                        let server_info_ui = if let Ok(info) = connection.store.server_info() {
                            text(info.name)
                        } else {
                            text("Failed loading server info from local data store!")
                        };

                        let channel_list_ui = if let Ok(x) =
                            connection.store.channels(Pagination::first(usize::MAX))
                        {
                            let y = Column::with_children(
                                x.focus
                                    .edges
                                    .iter()
                                    .map(|e| Element::from(e.node.get_name())),
                            );

                            scrollable(y)
                        } else {
                            text("Failed loading channel list")
                        };

                        let ui = column![
                            title,
                            server_info_ui,
                            row![channel_list_ui, active_channel_ui,]
                        ];

                        ui.into()
                    }
                }
            }
        }
    }

    fn theme(&self) -> Theme {
        Theme::Dracula
    }

    fn start(&mut self) -> Task<Message> {
        Task::none()
    }
}

type ConnectionHandle = Arc<ConnectionController>;
#[derive(Debug)]
struct ConnectionController {
    service: RpcServiceClient,
    store: ClientDataStore,
}
impl ConnectionController {
    async fn check_login(self: Arc<Self>) -> Result<bool> {
        if let Some(ClientSession {
            token: Some(token), ..
        }) = self.store.current_session()?.focus
        {
            Ok(self
                .service
                .validate_session(tarpc::context::current(), token)
                .instrument(info_span!("Validating stored session"))
                .await?
                .wrap_err("Error while talking to server")?)
        } else {
            Ok(false)
        }
    }

    async fn connect_to_ip(addr: impl ToSocketAddrs) -> Result<ConnectionHandle> {
        let mut transport = tarpc::serde_transport::tcp::connect(addr, Json::default);
        transport.config_mut().max_frame_length(usize::MAX);
        let service =
            RpcServiceClient::new(tarpc::client::Config::default(), transport.await?).spawn();
        let data = service
            .get_server_info(tarpc::context::current())
            .instrument(info_span!("Asking server for server data"))
            .await?
            .wrap_err("Error while talking to server")?;

        let store = open_client_db(data)?;
        Ok(Arc::new(ConnectionController { service, store }))
    }

    pub async fn register_login(self: Arc<Self>, username: String, password: String) -> Result<()> {
        let user_key = self
            .service
            .register_user(
                tarpc::context::current(),
                username.to_owned(),
                password.to_owned(),
            )
            .instrument(info_span!("Asking server for new user"))
            .await?
            .wrap_err("Error while talking to server")??;

        let client_session = ClientSession {
            user_key,
            token: None,
        };
        self.store.sync_users([Edge {
            node: User::new(username.to_owned()),
            cursor: user_key,
        }])?;
        self.store
            .set_current_session(client_session.clone())
            .wrap_err("Error while writing to local database")?;

        self.login_internal(client_session, &password).await
    }

    async fn login_internal(&self, session: ClientSession, password: &str) -> Result<()> {
        let user_key = session.user_key;
        let token = self
            .service
            .authenticate_session(tarpc::context::current(), user_key, password.into())
            .instrument(info_span!("Authenticating with server using credentials"))
            .await?
            .wrap_err("Error while talking to server")?;

        let session = ClientSession {
            user_key: user_key,
            token: Some(token),
        };
        self.store
            .set_current_session(session.clone())
            .wrap_err("Error while writing to local database")?;
        Ok(())
    }

    pub async fn login(self: Arc<Self>, username: String, password: String) -> Result<()> {
        let session = match self.store.current_session()?.focus {
            Some(session) => session,
            None => {
                let user_key = self
                    .service
                    .get_user_id_from_name(tarpc::context::current(), username.to_owned())
                    .instrument(info_span!("Asking server for existing user"))
                    .await?
                    .wrap_err("Error while talking to server")?;
                ClientSession {
                    user_key,
                    token: None,
                }
            }
        };
        self.login_internal(session, &password).await
    }
}
