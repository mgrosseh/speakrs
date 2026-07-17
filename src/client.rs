use std::{io::{Read, Write}, net::TcpStream};

use bytes::{BufMut, Bytes, BytesMut};

use crate::common::Arguments;


pub(crate) fn run(args: Arguments) {
    tui(args);
}

fn tui(args: Arguments) {
    // check if local command wants to be send, send
    // command. fetch commands with get if any


    let address = format!("{}:{}", args.server_ip, args.server_tcp_port);
    println!("Sending data to address: {}", address);
    let mut stream = TcpStream::connect(address).expect("Couldn't connect to the server...");

    let buf = "GET / HTTP/1.1".as_bytes();
    let write = stream.write_all(buf);
    if let Err(x) = write {
        println!("Error: {}", x);
    }
    let mut out: [u8; _] = [0; 128];
    let read = stream.read(&mut out);
    match read {
        Err(x) => println!("Error: {}", x),
        Ok(count) => println!("Read {} bytes: {}", count, String::from_utf8_lossy(&out)),
    }


}

// commands
// - ask create channel
// - send message
// - ask delete message
// - fetch message in channel

enum Command {
    
}

struct CommandCodec {
    encoder: fn(Command, &mut BytesMut),
    decoder: fn(&Bytes),
}

struct Common {
    //commands: 
}
