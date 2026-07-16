use std::{fs, io::{BufRead, BufReader, Write}, net::{TcpListener, TcpStream}, thread, time::Duration};

use crate::common::{Arguments, ThreadPool};

pub(crate) fn run(args: Arguments) {
    web_server(args);
}

fn web_server(args: Arguments) {
    tcp_server(args);
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

    match request_line.as_str() {
        "GET / HTTP/1.1" => {
            let response = response_serve_page("web/home.html");
            stream.write_all(response.as_bytes()).unwrap();
        }
        "GET /sleep HTTP/1.1" => {
            thread::sleep(Duration::from_secs(5));
            let response = response_serve_page("web/home.html");
            stream.write_all(response.as_bytes()).unwrap();

        }
        _ => {
            let response = response_serve_page_with_code("web/404.html", "404", "NOT FOUND");

            stream.write_all(response.as_bytes()).unwrap();
        }
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
