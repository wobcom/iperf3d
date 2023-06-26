use std::collections::HashMap;
use std::error::Error;
use std::process::{Child, Command};
use std::str;
use std::sync::Arc;
use std::time::Instant;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::time::{sleep, Duration};

use async_mutex::Mutex;

const PORT_REQUEST_MSG: &str = "iperf3d-port-request;";

pub async fn run(
    bind_address: &String,
    port: u16,
    iperf3_params: Vec<String>,
    start_port: u16,
    end_port: u16,
    max_age_seconds: u64,
) -> Result<(), Box<dyn Error>> {
    let addr = format!("{}:{}", bind_address, port);
    let listener = TcpListener::bind(&addr)
        .await
        .expect("Failed to bind to address");
    println!("Listening on: {}", addr);

    let state = State::new(start_port, end_port, iperf3_params)?;
    let state = Arc::new(Mutex::new(state));
    let cleanup_state = state.clone();

    tokio::spawn(async move {
        loop {
            sleep(Duration::from_secs(5)).await;
            let mut guard = cleanup_state.lock().await;
            guard.cleanup(max_age_seconds);
            drop(guard);
        }
    });

    loop {
        let (mut socket, _) = listener.accept().await?;
        let thread_state = state.clone();

        tokio::spawn(async move {
            let mut buf = vec![0; 1024];

            loop {
                let n = socket
                    .read(&mut buf)
                    .await
                    .expect("Failed to read data from socket");

                if n == 0 {
                    return;
                }

                let request = str::from_utf8(&buf[0..n]).expect("Failed to parse request");
                print!("{}", request);
                if request.starts_with(PORT_REQUEST_MSG) {
                    println!("Received port request");

                    let mut guard = thread_state.lock().await;
                    let port = guard.spawn_iperf3_server();
                    drop(guard);

                    match port {
                        Some(port) => {
                            socket
                                .write_all(format!("iperf3d-port: {};\n", port).as_bytes())
                                .await
                                .expect("Failed to write data to socket");
                        }
                        None => {
                            socket
                                .write_all("no-free-ports;\n".as_bytes())
                                .await
                                .expect("Failed to write data to socket");
                        }
                    }
                    socket.shutdown().await.expect("Failed to shutdown socket");
                    return;
                } else {
                    socket
                        .write_all("unknown-request;\n".as_bytes())
                        .await
                        .expect("Failed to write data to socket");
                    socket.shutdown().await.expect("Failed to shutdown socket");
                    return;
                }
            }
        });
    }
}

struct Iperf3Instance {
    spawn_time: Instant,
    port: u16,
    child: Option<Child>,
}

struct State {
    iperf3_instances: HashMap<u16, Iperf3Instance>,
    start_port: u16,
    end_port: u16,
    iperf3_params: Vec<String>,
}

impl State {
    fn new(
        start_port: u16,
        end_port: u16,
        iperf3_params: Vec<String>,
    ) -> Result<Self, &'static str> {
        if end_port < start_port {
            Err("End port must be bigger than start port")
        } else {
            let hash_map: HashMap<u16, Iperf3Instance> = HashMap::new();
            Ok(Self {
                iperf3_instances: hash_map,
                start_port,
                end_port,
                iperf3_params,
            })
        }
    }

    fn cleanup(&mut self, max_age_seconds: u64) {
        let mut ports_to_release: Vec<u16> = Vec::new();

        let used_ports = &mut self.iperf3_instances;

        let may_finished_instances: Vec<&mut Iperf3Instance> = used_ports
            .iter_mut()
            .map(|(_, instance)| instance)
            .filter(|instance| instance.spawn_time.elapsed().as_secs() < max_age_seconds)
            .collect();

        for instance in may_finished_instances {
            if let Some(child) = instance.child.as_mut() {
                let exited = child.try_wait().expect("Error to wait for iperf3 child");
                // try wait returns some if the process has finished
                if exited.is_some() {
                    instance.child.take();
                    ports_to_release.push(instance.port);
                }
            }
        }

        let too_old_instances: Vec<&mut Iperf3Instance> = used_ports
            .iter_mut()
            .map(|(_, instance)| instance)
            .filter(|instance| instance.spawn_time.elapsed().as_secs() >= max_age_seconds)
            .collect();

        for instance in too_old_instances {
            if let Some(child) = instance.child.as_mut() {
                child.kill().expect("Error killing iperf3 child");
                child.wait().expect("Error to wait for killed iperf3 child");
                instance.child.take();
            }
            ports_to_release.push(instance.port);
        }

        for port in ports_to_release {
            used_ports.remove(&port);
        }
    }

    fn get_next_free_port(&self) -> Option<u16> {
        let used_ports = &self.iperf3_instances;

        if used_ports.is_empty() {
            // if there are no used ports then the start port is the next free port
            return Some(self.start_port);
        }

        let mut ports: Vec<u16> = used_ports.into_iter().map(|(port, _)| *port).collect();

        ports.sort();

        if ports[0] != self.start_port {
            // if the first item in the sorted list is not the start port then
            // the start port is the next free port
            return Some(self.start_port);
        }

        let ports_len = ports.len() as u16;

        // check if the ports are exhausted
        if ports_len == (self.end_port - self.start_port + 1) {
            return None;
        }

        // try to find a hole
        for (i, port) in ports.into_iter().enumerate() {
            let i = (i + (self.start_port as usize)) as u16;

            if port > i {
                // in this case we found a hole and we use it
                return Some(i);
            }
        }

        // if there are no holes return the length as it is the next free number
        Some(ports_len + self.start_port)
    }

    fn spawn_iperf3_server(&mut self) -> Option<u16> {
        let port = self.get_next_free_port();

        if port.is_none() {
            return None;
        }

        let port = port.unwrap();

        let iperf3_child = Command::new("iperf3")
            .arg("-s")
            .arg("-1")
            .arg("-p")
            .arg(port.to_string())
            .args(&self.iperf3_params)
            .spawn()
            .expect("Failed to spawn iperf3 server");

        let instance = Iperf3Instance {
            spawn_time: Instant::now(),
            port: port,
            child: Some(iperf3_child),
        };

        self.iperf3_instances.insert(port, instance);

        Some(port)
    }
}
