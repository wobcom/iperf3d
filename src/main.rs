use clap::{arg, command, ArgAction};
use std::error::Error;

mod client;
mod consts;
mod server;

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
                .required_unless_present("client")
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
            arg!(--dstart <PORT> "First port of the dynamic port range (the bind port of iperf3d must not be in this range)")
                .action(ArgAction::Set)
                .value_parser(clap::value_parser!(u16))
                .default_value("7000")
                .default_missing_value("7000")
        )
        .arg(
            arg!(--dend <PORT> "Last port of the dynamic port range (the bind port of iperf3d must not be in this range)")
                .action(ArgAction::Set)
                .value_parser(clap::value_parser!(u16))
                .default_value("7999")
                .default_missing_value("7999")
        )
        .arg(
            arg!(--"max-age" <SECONDS> "Maximum time a single iperf3 server is allowed to run")
                .action(ArgAction::Set)
                .value_parser(clap::value_parser!(u64))
                .default_value("300")
                .default_missing_value("300"),
        )
        .arg(
            arg!(--"iperf3-path" <PATH> "Path to the iperf3 executable (only required if iperf3 is not in $PATH)")
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

    let final_iperf3_params: Vec<String>;

    if let Some(iperf3_params) = iperf3_params {
        let iperf3_params: Vec<String> = iperf3_params.cloned().collect();
        final_iperf3_params = iperf3_params;
    } else {
        let iperf3_params: Vec<String> = vec![];
        final_iperf3_params = iperf3_params;
    }

    let port: u16 = *matches.get_one("port").unwrap();

    let iperf3_path: Option<&String> = matches.get_one("iperf3-path");
    let iperf3_path = match iperf3_path {
        None => "iperf3".to_string(),
        Some(path) => path.to_string(),
    };

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
        let start_port: u16 = *matches.get_one("dstart").unwrap();
        let end_port: u16 = *matches.get_one("dend").unwrap();
        if start_port <= port && port <= end_port {
            println!("The bind port of iperf3d must not be in the dynamic port range. Exiting.");
            return Ok(());
        }
        let max_age_seconds: u64 = *matches.get_one("max-age").unwrap();
        server::run(
            final_bind_address,
            port,
            iperf3_path,
            final_iperf3_params.to_owned(),
            start_port,
            end_port,
            max_age_seconds,
        )
        .await
    } else {
        // client mode
        let target: &String = matches
            .get_one("client")
            .expect("Target must be set in client mode");
        client::run(target, port, iperf3_path, final_iperf3_params).await
    }
}
