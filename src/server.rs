use std::{fs, io::{BufRead, BufReader, Read, Write}, net::{Shutdown, TcpListener, TcpStream, UdpSocket}, thread, time::Duration};

use crate::common::{Arguments, ThreadPool};

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
        let mut stream = stream.unwrap();
        let mut data = [0_u8; 50];
        while match stream.read(&mut data) {
            Ok(size) => {
                stream.write_all(&data[0..size]).unwrap();
                true
            },
            Err(_) => {
                println!("An error occurred, terminating connection with {}", stream.peer_addr().unwrap());
                stream.shutdown(Shutdown::Both).unwrap();
                false
            }
        } {}

        pool.execute(move || {  
            handle_connection(args, stream);
        });

    }

}

fn handle_connection(args: Arguments, mut stream: TcpStream) {
    let buf_reader = BufReader::new(&stream);
    let http_request: Vec<String> = buf_reader
            .lines()
            .map(|result| result.unwrap())
            .take_while(|line| !line.is_empty())
            .collect();

    if args.verbose {
        println!("Request: {http_request:#?}");
    }
    let request_line = http_request.first().unwrap();

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
            ("/sleep", "HTTP/1.1") => {
                thread::sleep(Duration::from_secs(5));
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
