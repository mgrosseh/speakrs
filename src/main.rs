mod common;
mod client;
mod server;

use std::env;

use crate::common::Arguments;

fn main() {
    let args: Vec<String> = env::args().collect();
    let args: Option<Arguments> = common::Arguments::parse(&args[1..]);
    if args.is_none() {
        return;
    }
    let args: Arguments = args.unwrap();
    match args.mode {
        common::Mode::Client => client::run(args),
        common::Mode::Server => server::run(args),
    }
}
