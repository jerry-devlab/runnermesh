use std::{env, process, time::Duration};

use runnermesh::{execute_cli, LocalAgentTransport};

fn main() {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    let transport = LocalAgentTransport::new(Duration::from_secs(2));

    match execute_cli(&arguments, &transport, env!("CARGO_PKG_VERSION")) {
        Ok(output) => println!("{output}"),
        Err(error) => {
            eprintln!("runnermesh: {error}");
            process::exit(2);
        }
    }
}
