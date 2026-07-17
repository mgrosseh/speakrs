use std::{fs, io::{BufRead, BufReader, Read, Write}, net::{Shutdown, TcpListener, TcpStream, UdpSocket}, thread, time::{Duration, SystemTime}};

use nom::AsBytes;

use crate::common::{self, Arguments, NetworkCodable, ThreadPool};

pub(crate) fn run(args: Arguments) {
    web_server(args);
}

fn web_server(args: Arguments) {
    tcp_server(args);
    udp_server(args); // TODO: currently never called because of loop, use threads
}

fn udp_server(args: Arguments) { // TODO:
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

fn tcp_server(args: Arguments) {
    let address = format!("{}:{}", args.server_ip, args.server_tcp_port);

    let listener = TcpListener::bind(address).unwrap();
    let pool = ThreadPool::with_name(4, "web_request_listener", args);

    if !args.quiet {
        let address = format!("{}:{}", args.server_ip, args.server_tcp_port);
        println!("Serving on address: {address}.")
    }

    for stream in listener.incoming() {
        let stream = stream.unwrap();

        pool.execute(move || {  
            handle_connection(args, stream);
        });

    }

}

fn handle_connection(args: Arguments, stream: TcpStream) {
    let mut buf_reader = BufReader::new(&stream);
    let mut request_line: String = String::new();
    buf_reader.read_line(&mut request_line).unwrap();
    let request_line = request_line.trim(); // ensure trailing newline's (etc) is removed

    if args.verbose {
        println!("Request: {request_line:#?}");
    }

    if request_line.contains("HTTP/1.1") {
        handle_http_request(request_line, &stream, buf_reader);
    }
    else if request_line.starts_with(common::PROTOCOL_KEYWORD) {
        handle_speakrs_request(args, request_line, &stream, buf_reader);
    }
}

// ==============================
// => SPEAKRS
// ==============================

fn handle_speakrs_request(args: Arguments, request_line: &str, mut stream: &TcpStream, mut buf_reader: BufReader<&TcpStream>) {
    let mut rest = Vec::new();
    buf_reader.read_to_end(&mut rest).unwrap();
    let request = format!("{}{}", request_line, str::from_utf8(rest.as_bytes()).unwrap());

    let wrapped_command = common::Protocol::decode(request.as_bytes());
    if let Err(x) = wrapped_command {
        if args.verbose {
            println!("Failed to parse command: `{}` || error: `{}`", request, x);
        }
        return;
    }
    let (rest, command) = wrapped_command.unwrap();
    match command {
        common::Protocol::CreateChannelRequest(cmd) => println!("channel request: {} {}", cmd.name, cmd.desc),
        common::Protocol::SendMessage(cmd) => println!("message request: {} {} {}", 
            cmd.timestamp.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs(),
            cmd.user, cmd.content),
        common::Protocol::GetMessage(cmd) => todo!(),
        common::Protocol::DeleteMessageRequest(cmd) => todo!(),
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
