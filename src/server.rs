use std::{fs, io::{BufRead, BufReader, Read, Write}, net::{TcpListener, TcpStream, UdpSocket}, sync::{Arc, Mutex}, time::SystemTime};

use nom::AsBytes;

use crate::{common::{self, Arguments, ThreadPool}, protocol::{GetDataProtocol, MessageIdsProtocol, NewDataProtocol, Protocol}};


pub(crate) fn run(args: Arguments) {
    web_server(args);
}

fn web_server(args: Arguments) {
    let server = ServerData::new();
    tcp_server(args, server.clone());
    udp_server(args, server.clone()); // TODO: currently never called because of loop, use threads
}

fn udp_server(args: Arguments, server: ServerData) { // TODO:
    let address = format!("{}:{}", args.server_ip, args.server_udp_port);
    let socket = UdpSocket::bind(address).unwrap();

    let mut buf = [0; 10];
    let (amt, _src) = socket.recv_from(&mut buf).unwrap();

    let buf = &mut buf[..amt];
    print!("Received upd data: ");
    for c in buf {
        print!("{}", c);
    }
    println!();

}

fn tcp_server(args: Arguments, server: ServerData) {
    let address = format!("{}:{}", args.server_ip, args.server_tcp_port);

    let listener = TcpListener::bind(address).unwrap();
    let pool = ThreadPool::with_name(4, "web_request_listener", args);

    if !args.quiet {
        let address = format!("{}:{}", args.server_ip, args.server_tcp_port);
        println!("Serving on address: {address}.")
    }

    for stream in listener.incoming() {
        let stream = stream.unwrap();
        let server = server.clone();

        pool.execute(move || {  
            handle_connection(args, server, stream);
        });

    }

}

fn handle_connection(args: Arguments, server: ServerData, stream: TcpStream) {
    let mut buf_reader = BufReader::new(&stream);

    let mut lookahead = [0_u8; common::PROTOCOL_KEYWORD.len()];
    let read = buf_reader.read(&mut lookahead).unwrap();
    let lookahead = &lookahead[..read];
    if common::PROTOCOL_KEYWORD.eq(str::from_utf8(lookahead).unwrap()) {
        handle_speakrs_request(args, server, common::PROTOCOL_KEYWORD, &stream, buf_reader);
        return;
    }

    let mut request_line: String = str::from_utf8(lookahead).unwrap().to_string();
    buf_reader.read_line(&mut request_line).unwrap();
    let request_line = request_line.trim(); // ensure trailing newline's (etc) is removed

    if args.verbose {
        println!("Request: {request_line:#?}");
    }

    if request_line.contains("HTTP/1.1") {
        handle_http_request(request_line, &stream, buf_reader);
    }
}

// ==============================
// => Server Data
// ==============================

#[derive(Clone)]
struct ServerData {
    db: Arc<Mutex<common::Server>>,
}

impl ServerData {
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
// => SPEAKRS
// ==============================

fn handle_speakrs_request(args: Arguments, mut server: ServerData, request_line: &str, _stream: &TcpStream, mut buf_reader: BufReader<&TcpStream>) {
    let mut rest = Vec::new();
    buf_reader.read_until(common::PROTOCOL_END_CHAR as u8, &mut rest).unwrap();
    let request = format!("{}{}", request_line, str::from_utf8(rest.as_bytes()).unwrap());

    if args.verbose {
        println!("Request (Speakrs): {request:#?}");
    }

    let command = Protocol::parse_protocol_with_error_handling(request, args.verbose);
    if command.is_none() {
        return; // error reporting in function above
    }
    let command = command.unwrap();

    match command {
        Protocol::NewData(protocol) => match protocol {
            NewDataProtocol::Channel(cmd) => {
                let mut sd = server.lock_db().unwrap_or_else(|_| panic!("While handling CreateChannel Protocol: Could not acquire lock on server database."));
                match sd.add_channel(cmd.name.clone(), cmd.desc.clone()) {
                    Err(x) => {
                        println!("Encountered server error while trying to create channel \"{}\": {}", cmd.name, x);
                    },
                    Ok(id) => {
                        println!("Created channel \"{}\" with id {} and description: |||{}|||", cmd.name, id, cmd.desc);
                    }
                }
            },
            NewDataProtocol::Message(cmd) => {
                let mut sd = server.lock_db().unwrap_or_else(|_| panic!("While handling SendMessage Protocol: Could not acquire lock on server database."));
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
            NewDataProtocol::User(cmd) => {
                let mut sd = server.lock_db().unwrap_or_else(|_| panic!("While handling Adduser Protocol: Could not acquire lock on server database."));
                match sd.add_user(cmd.username.clone()) {
                    Err(x) => {
                        println!("Encountered server error while trying to create user \"{}\": {}", cmd.username, x);
                    },
                    Ok(id) => {
                        println!("Created user \"{}\" with id {}.", cmd.username, id);
                    }
                }

            },
        }
        Protocol::RegisterData(cmd) => todo!(), // TODO: server does not accept incoming register commands -- send back error
        Protocol::GetData(cmd) => match cmd {
            GetDataProtocol::Message(MessageIdsProtocol(channel_id, message_ids)) => {
                // TODO: locking server db could be a problem
                let sd = server.lock_db().unwrap_or_else(|_| panic!("While handling GetData Channel Protocol: Could not acquire lock on server database."));
                match sd.get_messages(channel_id, &message_ids[..]) {
                    Err(server_error) => todo!(),
                    Ok(messages) => todo!(),
                }
            },
            GetDataProtocol::User(user_ids) => todo!(),
            GetDataProtocol::Channel(channel_ids) => todo!(),
        },
        Protocol::DeleteData(cmd) => todo!(),
        Protocol::NewData(new_data_protocol) => todo!(),
        Protocol::SendData(send_data_protocol) => todo!(),
        Protocol::ServerError(server_error) => todo!(),
    }
}

// ==============================
// => HTTP
// ==============================

fn handle_http_request(request_line: &str, mut stream: &TcpStream, buf_reader: BufReader<&TcpStream>) {

    // read stream fully
    let _request: Vec<String> = buf_reader
            .lines()
            .map(|result| result.unwrap())
            .take_while(|line| !line.is_empty())
            .collect();

    if request_line.starts_with("GET ") {
        let mut split = request_line.split(' ');
        let _ = split.next().unwrap(); // always GET
        let http_address = split.next().unwrap();
        let http_version = split.next().unwrap();


        match (http_address, http_version) {
            ("/",  "HTTP/1.1") => {
                let response = response_serve_page("web/home.html");
                stream.write_all(response.as_bytes()).unwrap();
            }
            (addr, "HTTP/1.1") if addr.starts_with("/text_channel/") => {
                //let subaddress = addr.strip_prefix("/text_channel/"); // TODO:
            }
            _ => {
                let response = response_serve_page_with_code("web/404.html", "404", "NOT FOUND");

                stream.write_all(response.as_bytes()).unwrap();
            }
        }
    }
    else {
        let response = response_serve_page_with_code("web/404.html", "404", "NOT FOUND");
        stream.write_all(response.as_bytes()).unwrap();
    }
}

fn response_serve_page(address: &str) -> String {
    response_serve_page_with_code(address, "200", "OK")
}
fn response_serve_page_with_code(address: &str, code: &str, html_response: &str) -> String {
    let status_line = "HTTP/1.1";
    let contents = fs::read_to_string(address).unwrap();
    let lenght = contents.len();

    format!("{status_line} {code} {html_response}\r\nContent-Length: {lenght}\r\n\r\n{contents}")
}
