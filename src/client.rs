use std::{io::{BufRead, BufReader, Read, Write}, net::TcpStream, sync::{Arc, Mutex}, time::SystemTime};

use crate::{common::{self, Arguments, ChannelId, UserId}, protocol::Protocol};


pub(crate) fn run(args: Arguments) {
    tui(args);
}

fn tui(args: Arguments) {


    let address = format!("{}:{}", args.server_ip, args.server_tcp_port);
    println!("Sending data to address: {}", address);
    let mut stream = TcpStream::connect(address).expect("Couldn't connect to the server...");


    let command = Protocol::create_channel("Channel Name".to_string(), "A channel.".to_string());
    if let Err(x) = command.send_protocol(&stream) && args.verbose {
        println!("Write Error: {}", x);
    }

    let command = Protocol::send_message(ChannelId::default(), SystemTime::now(), UserId::default(), "A test message".to_string());
    if let Err(x) = command.send_protocol(&stream) && args.verbose {
        println!("Write Error: {}", x);
    }

    let mut out: [u8; _] = [0; 128];
    let read = stream.read(&mut out);
    match read {
        Err(x) => println!("Read Error: {}", x),
        Ok(count) => println!("Read {} bytes: {}", count, str::from_utf8(&out).unwrap()),
    }
}

// ==============================
// => Client Data
// ==============================
#[derive(Clone)]
struct ClientData {
    db: Arc<Mutex<common::Server>>,
}

impl ClientData {
    fn new() -> Self {
        let db = Arc::new(Mutex::new(common::Server::new()));
        Self {
            db
        }
    }

    fn lock_db(&mut self) -> Result<std::sync::MutexGuard<'_, common::Server>, std::sync::PoisonError<std::sync::MutexGuard<'_, common::Server>>> {
        self.db.lock()
    }
}

// ==============================
// => Speakrs
// ==============================
fn handle_connection(args: Arguments, server: ClientData, stream: TcpStream) {
    let mut buf_reader = BufReader::new(&stream);

    let mut request = Vec::new();
    buf_reader.read_until(common::PROTOCOL_END_CHAR as u8, &mut request).unwrap();
    let request = String::from_utf8(request).unwrap();

    if !request.starts_with(common::PROTOCOL_KEYWORD) {
        if args.verbose {
            println!("Unrecognized incoming request: `{}`", request);
        }
        return;
    }
    handle_speakrs_request(args, server, &stream, request);
}

fn handle_speakrs_request(args: Arguments, mut client: ClientData, stream: &TcpStream, request: String) {
    if args.verbose {
        println!("Request (Speakrs): {request:#?}");
    }

    let command = Protocol::parse_protocol_with_error_handling(request, args.verbose);
    if command.is_none() {
        return; // error reporting in function above
    }
    let command = command.unwrap();


    match command {
        Protocol::AddChannel(cmd) => {
            let mut sd = client.lock_db().unwrap_or_else(|_| panic!("While handling CreateChannel Protocol: Could not acquire lock on server database."));
            match sd.add_channel(cmd.name.clone(), cmd.desc.clone()) {
                Err(x) => {
                    println!("Encountered server error while trying to create channel \"{}\": {}", cmd.name, x);
                },
                Ok(id) => {
                    println!("Created channel \"{}\" with id {} and description: |||{}|||", cmd.name, id, cmd.desc);
                }
            }
        },
        Protocol::AddMessage(cmd) => {
            let mut sd = client.lock_db().unwrap_or_else(|_| panic!("While handling SendMessage Protocol: Could not acquire lock on server database."));
            let time_in_secs = cmd.timestamp.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs();

            match sd.send_message(cmd.channel, cmd.timestamp, cmd.user, cmd.content.clone()) {
                Err(x) => {
                    println!("Encountered server error while trying to send message by user {} send at {} in channel {}: {}", cmd.user, time_in_secs, cmd.channel, x);
                },
                Ok(id) => {
                    println!("Send message {} by user {} at {} in channel {}: {}", id, cmd.user, time_in_secs, cmd.channel, cmd.content);
                }
            }
        },
        Protocol::AddUser(cmd) => {
            let mut sd = client.lock_db().unwrap_or_else(|_| panic!("While handling Adduser Protocol: Could not acquire lock on server database."));
            match sd.add_user(cmd.username.clone()) {
                Err(x) => {
                    println!("Encountered server error while trying to create user \"{}\": {}", cmd.username, x);
                },
                Ok(id) => {
                    println!("Created user \"{}\" with id {}.", cmd.username, id);
                }
            }

        },
        Protocol::RegisterData(cmd) => todo!(),
        Protocol::GetData(cmd) => todo!(),
        Protocol::DeleteData(cmd) => todo!(),
        Protocol::NewData(new_data_protocol) => todo!(),
        Protocol::SendData(send_data_protocol) => todo!(),
        Protocol::ServerError(server_error) => todo!(),
    }
}
