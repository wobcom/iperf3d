use std::error::Error;
use std::process::Command;

pub fn run(
    target: &String,
    port: u16,
    iperf3_params: Vec<String>,
) -> Result<(), Box<dyn Error>> {
    let mut iperf3_child = Command::new("iperf3")
        .arg("-c")
        .arg(target)
        .arg("-p")
        .arg(port.to_string())
        .args(iperf3_params)
        .spawn()
        .expect("Failed to spawn iperf3 client");

    iperf3_child
        .wait()
        .expect("Failed to wait for iperf3 client");
    Ok(())
}
