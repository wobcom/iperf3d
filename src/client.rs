use std::error::Error;
use std::process::Command;
use std::str;

use crate::consts::*;

use regex::Regex;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

pub async fn run(
    target: &String,
    port: u16,
    iperf3_params: Vec<String>,
) -> Result<(), Box<dyn Error>> {
    let address = format!("{}:{}", target, port);

    let mut stream = TcpStream::connect(address).await?;

    stream.write(PORT_REQUEST_MSG.as_bytes()).await?;

    let mut buf = vec![0; 1024];
    let n = stream
        .read(&mut buf)
        .await
        .expect("Failed to read data from stream");

    stream.shutdown().await?;

    let response = str::from_utf8(&buf[0..n])
        .expect("Failed to parse response")
        .trim();

    if response.starts_with(NO_FREE_PORTS_MSG) {
        println!("All dynamic iperf3 ports are currently in use, please try again later.");
        return Ok(());
    }

    let re = Regex::new(format!(r"{}(\d+){}", PORT_RESPONSE_MSG_START, PORT_RESPONSE_MSG_END).as_str()).unwrap();

    let captures = re.captures(response);

    if captures.is_none() {
        println!("Invalid response from server, exiting.");
        return Ok(());
    }

    let captures = captures.unwrap();
    let iperf3_port = &captures[1];

    let mut iperf3_child = Command::new("iperf3")
        .arg("-c")
        .arg(target)
        .arg("-p")
        .arg(iperf3_port)
        .args(iperf3_params)
        .spawn()
        .expect("Failed to spawn iperf3 client");

    iperf3_child
        .wait()
        .expect("Failed to wait for iperf3 client");
    Ok(())
}
