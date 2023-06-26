# iperf3d

`iperf3d` is a [iperf3](https://github.com/esnet/iperf) client and server wrapper for dynamic server ports. It aims to be compatible with the `iperf3` command line flags to be a replacement for existing applications that use a `iperf3` client and server.

The `iperf3d` server listens on a control port (default 6201) for connections of a `iperf3d` client. When a client connects, the server will start a `iperf3` server in oneshot mode on the next free port in the dynamic port range (default 7000-7999) and tell the port to the client. The client then starts a normal `iperf3` client to connect to the received port. The `iperf3` server will automatically terminate after the run because of the oneshot-mode.

## Usage
```
Usage: iperf3d [OPTIONS] [iperf3_params]...

Arguments:
  [iperf3_params]...  Arguments that will be passed to iperf3

Options:
  -c, --client <TARGET>     Enables the client mode to server <TARGET>
  -s, --server              Enables the server mode
  -p, --port <PORT>         Port to listen or connect to [default: 6201]
  -B, --bind <ADDRESS>      Address to bind to, if set, it will also be passed to iperf3
      --dstart <PORT>       First port of the dynamic port range (the bind port of iperf3d must not be in this range) [default: 7000]
      --dend <PORT>         Last port of the dynamic port range (the bind port of iperf3d must not be in this range) [default: 7999]
      --max-age <SECONDS>   Maximum time a single iperf3 server is allowed to run [default: 300]
      --ip-limit <NUMBER>   Number of concurrent sessions allowed from the same IP [default: 3]
      --iperf3-path <PATH>  Path to the iperf3 executable (only required if iperf3 is not in $PATH)
  -h, --help                Print help
  -V, --version             Print version
```

## Server usage

Like `iperf3`, you can start a `iperf3d` server with:
```
iperf3d -s
```
The server will then bind to the default port (6201) and will use the default dynamic port range for dynamic `iperf3` servers (7000-7999).

### Optional server options

Like in `iperf3`, you can pass a bind-address with `-B` to `iperf3d` so that it only binds to the given address. The given address will also be passed to the `iperf3` servers.

To change the ports, you can use `-p` or `--port` to change the control port (like in `iperf`) and you can use `--dstart` and `--dend` for setting the start and the end port of the dynamic port range.

You can also pass additional `iperf3` arguments just by putting them behind the `iperf3d` options.

### DoS protection

As a Denial-of-Service protection, the maximum lifetime of dynamic `iperf3` server and the maximum dynamic `iperf3` servers per client IP are limited.

The default maximum lifetime is 300 seconds (5 minutes) and the default limit for concurrent sessions for one IP is 3. You can adjust this limits with the `--max-age` and `--ip-limit` options.

## Client usage

Like `iperf3`, you can connect to a `iperf3d` server with:
```
iperf3d -c [address]
```
The client will then perform a `iperf3` to the dynamic server received from the `iperf3d` server.

You can also specify a different port of the `iperf3d` server by using `-p` or `--port`.

Like in the server mode, you can also pass additional `iperf3` arguments just by putting them behind the `iperf3d` options, for example for limiting the bitrate:
```
iperf3d -c [address] -b 100M
```