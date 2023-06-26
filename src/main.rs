use clap::{arg, command, ArgAction};
use std::error::Error;

mod client;
mod server;
mod consts;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let matches = command!()
        .arg(
            arg!(-c --client <TARGET> "Enables the client mode to server <TARGET>")
                .action(ArgAction::Set)
                .conflicts_with("server")
                .required_unless_present("server"),
        )
        .arg(
            arg!(-s --server ... "Enables the server mode")
                .action(ArgAction::SetTrue)
                .conflicts_with("client")
                .required_unless_present("client"),
        )
        .arg(
            arg!(-p --port <PORT> "Port to listen or connect to")
                .action(ArgAction::Set)
                .value_parser(clap::value_parser!(u16))
                .default_value("6201")
                .default_missing_value("6201"),
        )
        .arg(
            arg!(-B --bind <ADDRESS> "Address to bind to, if set, it will also be passed to iperf3")
                .action(ArgAction::Set)
        )
        .arg(
            arg!(<iperf3_params> ... "Arguments that will be passed to iperf3")
                .trailing_var_arg(true)
                .required(false)
                .allow_hyphen_values(true),
        )
        .get_matches();

    let iperf3_params = matches.get_many::<String>("iperf3_params");

    let port: u16 = *matches.get_one("port").unwrap();

    println!("Port {}", port);

    let final_iperf3_params: Vec<String>;

    if let Some(iperf3_params) = iperf3_params {
        let iperf3_params: Vec<String> = iperf3_params.cloned().collect();
        final_iperf3_params = iperf3_params;
    } else {
        let iperf3_params: Vec<String> = vec![];
        final_iperf3_params = iperf3_params;
    }

    let server_mode = matches.get_flag("server");

    if server_mode {
        let final_bind_address: &String;
        let bind_address = matches.get_one("bind");
        let bind_all = "[::]".to_string();
        if let Some(bind_address) = bind_address {
            final_bind_address = bind_address;
        } else {
            final_bind_address = &bind_all;
        }
        server::run(final_bind_address, port, final_iperf3_params.to_owned(), 10000, 20000, 300).await
    } else {
        // client mode
        let target: &String = matches
            .get_one("client")
            .expect("Target must be set in client mode");
        client::run(target, port, final_iperf3_params)
    }
}
