use std::{io::{Read, Write}, net::TcpStream};

use bytes::{BufMut, Bytes, BytesMut};

use crate::common::{self, Arguments, CreateChannelCommand, NetworkCodable, Protocol};


pub(crate) fn run(args: Arguments) {
    tui(args);
}

fn tui(args: Arguments) {


    let address = format!("{}:{}", args.server_ip, args.server_tcp_port);
    println!("Sending data to address: {}", address);
    let mut stream = TcpStream::connect(address).expect("Couldn't connect to the server...");

    let command = Protocol::create_channel("Channel Name".to_string(), "A channel.".to_string());

    let encoded = command.encode();
    let buf = encoded.as_bytes();
    let write = stream.write_all(buf);
    if let Err(x) = write {
        println!("Error: {}", x);
    }
    let mut out: [u8; _] = [0; 128];
    let read = stream.read(&mut out);
    match read {
        Err(x) => println!("Error: {}", x),
        Ok(count) => println!("Read {} bytes: {}", count, str::from_utf8(&out).unwrap()),
    }
}
