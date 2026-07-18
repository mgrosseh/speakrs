use std::{io::{Read, Write}, net::TcpStream, time::SystemTime};

use crate::common::{self, Arguments, ChannelId, NetworkCodable, Protocol, UserId};


pub(crate) fn run(args: Arguments) {
    tui(args);
}

fn tui(args: Arguments) {


    let address = format!("{}:{}", args.server_ip, args.server_tcp_port);
    println!("Sending data to address: {}", address);
    let mut stream = TcpStream::connect(address).expect("Couldn't connect to the server...");

    let command = Protocol::create_channel("Channel Name".to_string(), "A channel.".to_string());
    if let Err(x) = send_protocol(&stream, command) && args.verbose {
        println!("Write Error: {}", x);
    }

    let command = Protocol::send_message(ChannelId::default(), SystemTime::now(), UserId::default(), "A test message".to_string());
    if let Err(x) = send_protocol(&stream, command) && args.verbose {
        println!("Write Error: {}", x);
    }

    let mut out: [u8; _] = [0; 128];
    let read = stream.read(&mut out);
    match read {
        Err(x) => println!("Read Error: {}", x),
        Ok(count) => println!("Read {} bytes: {}", count, str::from_utf8(&out).unwrap()),
    }
}

fn send_protocol(mut stream: &TcpStream, protocol: Protocol) -> Result<(), std::io::Error> {
    let mut encoded = protocol.encode();
    encoded.push(common::PROTOCOL_END_CHAR);
    let buf = encoded.as_bytes();
    stream.write_all(buf)
}
