use std::collections::HashMap;
use std::error::Error;
use std::net::IpAddr;
use std::process::{Child, Command};
use std::str;
use std::sync::Arc;
use std::time::Instant;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::time::{sleep, Duration};

use async_mutex::Mutex;

use crate::consts::*;

pub async fn run(
    bind_address: &String,
    port: u16,
    iperf3_path: String,
    iperf3_params: Vec<String>,
    start_port: u16,
    end_port: u16,
    max_age_seconds: u64,
    max_instances_by_ip: u8,
) -> Result<(), Box<dyn Error>> {
    let addr = format!("{}:{}", bind_address, port);
    let listener = TcpListener::bind(&addr)
        .await
        .expect("Failed to bind to address");
    println!("Listening on: {}", addr);
    println!("Dynamic port range: {}-{}", start_port, end_port);

    let state = State::new(
        max_instances_by_ip,
        start_port,
        end_port,
        iperf3_path,
        iperf3_params,
        bind_address.clone(),
    )?;
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
                if request.starts_with(PORT_REQUEST_MSG) {
                    let peer_ip = socket
                        .peer_addr()
                        .expect("Failed to extract peer address")
                        .ip();

                    let mut guard = thread_state.lock().await;

                    if guard.get_instance_count_by_ip(peer_ip) >= guard.max_instances_by_ip as usize
                    {
                        socket
                            .write_all(format!("{}\n", IP_LIMIT_REACHED_MSG).as_bytes())
                            .await
                            .expect("Failed to write data to socket");
                        drop(guard);
                        socket.shutdown().await.expect("Failed to shutdown socket");
                        return;
                    }

                    let port = guard.spawn_iperf3_server(peer_ip);
                    drop(guard);

                    match port {
                        Some(port) => {
                            socket
                                .write_all(
                                    format!(
                                        "{}{}{}\n",
                                        PORT_RESPONSE_MSG_START, port, PORT_RESPONSE_MSG_END
                                    )
                                    .as_bytes(),
                                )
                                .await
                                .expect("Failed to write data to socket");
                        }
                        None => {
                            socket
                                .write_all(format!("{}\n", NO_FREE_PORTS_MSG).as_bytes())
                                .await
                                .expect("Failed to write data to socket");
                        }
                    }
                    socket.shutdown().await.expect("Failed to shutdown socket");
                    return;
                } else {
                    socket
                        .write_all(format!("{}\n", UNKNOWN_REQUEST_MSG).as_bytes())
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
    requested_by_ip: IpAddr,
    port: u16,
    child: Option<Child>,
}

struct State {
    iperf3_instances: HashMap<u16, Iperf3Instance>,
    max_instances_by_ip: u8,
    start_port: u16,
    end_port: u16,
    iperf3_path: String,
    iperf3_params: Vec<String>,
    bind_address: String,
}

impl State {
    fn new(
        max_instances_by_ip: u8,
        start_port: u16,
        end_port: u16,
        iperf3_path: String,
        iperf3_params: Vec<String>,
        bind_address: String,
    ) -> Result<Self, &'static str> {
        if end_port < start_port {
            Err("End port must be bigger than start port")
        } else {
            let hash_map: HashMap<u16, Iperf3Instance> = HashMap::new();
            Ok(Self {
                iperf3_instances: hash_map,
                max_instances_by_ip,
                start_port,
                end_port,
                iperf3_path,
                iperf3_params,
                bind_address,
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

    fn get_instance_count_by_ip(&self, ip: IpAddr) -> usize {
        let instances_by_ip: Vec<&Iperf3Instance> = self
            .iperf3_instances
            .iter()
            .map(|(_, instance)| instance)
            .filter(|instance| instance.requested_by_ip == ip)
            .collect();

        instances_by_ip.len()
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

    fn spawn_iperf3_server(&mut self, requested_by_ip: IpAddr) -> Option<u16> {
        let port = self.get_next_free_port();

        if port.is_none() {
            return None;
        }

        let port = port.unwrap();

        let mut iperf3_command = Command::new(&self.iperf3_path);
        let mut iperf3_command = iperf3_command
            .arg("-s")
            .arg("-1")
            .arg("-p")
            .arg(port.to_string());

        if self.bind_address != BIND_ALL_ADDRESS {
            iperf3_command = iperf3_command.arg("-B").arg(&self.bind_address)
        }

        iperf3_command = iperf3_command.args(&self.iperf3_params);

        let iperf3_child = iperf3_command
            .spawn()
            .expect("Failed to spawn iperf3 server");

        let instance = Iperf3Instance {
            spawn_time: Instant::now(),
            requested_by_ip,
            port: port,
            child: Some(iperf3_child),
        };

        self.iperf3_instances.insert(port, instance);

        Some(port)
    }
}
