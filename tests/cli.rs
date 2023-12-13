use anyhow::Context;
use rand::Rng;

use std::{
    io::{self, Read, Write},
    net::{TcpListener, TcpStream},
    str::FromStr,
    sync::{Arc, Barrier},
    time::Duration,
};
#[path = "../src/sendme_ticket.rs"]
mod sendme_ticket;

// binary path
fn sendme_bin() -> &'static str {
    env!("CARGO_BIN_EXE_sendme")
}

/// Read `n` lines from `reader`, returning the bytes read including the newlines.
///
/// This assumes that the header lines are ASCII and can be parsed byte by byte.
fn read_ascii_lines(mut n: usize, reader: &mut impl Read) -> io::Result<Vec<u8>> {
    let mut buf = [0u8; 1];
    let mut res = Vec::new();
    loop {
        if reader.read(&mut buf)? != 1 {
            break;
        }
        let char = buf[0];
        res.push(char);
        if char != b'\n' {
            continue;
        }
        if n > 1 {
            n -= 1;
        } else {
            break;
        }
    }
    Ok(res)
}

fn wait2() -> Arc<Barrier> {
    Arc::new(Barrier::new(2))
}

/// generate a random, non privileged port
fn random_port() -> u16 {
    rand::thread_rng().gen_range(10000u16..60000)
}

#[test]
fn provide_get_file() {
    let name = "somefile.bin";
    let data = vec![0u8; 100];
    // create src and tgt dir, and src file
    let src_dir = tempfile::tempdir().unwrap();
    let tgt_dir = tempfile::tempdir().unwrap();
    let src_file = src_dir.path().join(name);
    std::fs::write(&src_file, &data).unwrap();
    let mut provide = duct::cmd(
        sendme_bin(),
        ["provide", src_file.as_os_str().to_str().unwrap()],
    )
    .dir(src_dir.path())
    .env_remove("RUST_LOG") // disable tracing
    .stderr_to_stdout()
    .reader()
    .unwrap();
    let output = read_ascii_lines(4, &mut provide).unwrap();
    let output = String::from_utf8(output).unwrap();
    let ticket = output.split_ascii_whitespace().last().unwrap();
    let ticket = sendme_ticket::Ticket::from_str(ticket).unwrap();
    let get = duct::cmd(sendme_bin(), ["get", &ticket.to_string()])
        .dir(tgt_dir.path())
        .env_remove("RUST_LOG") // disable tracing
        .stderr_to_stdout()
        .run()
        .unwrap();
    assert!(get.status.success());
    let tgt_file = tgt_dir.path().join(name);
    let tgt_data = std::fs::read(&tgt_file).unwrap();
    assert_eq!(tgt_data, data);
}

// /// Tests the basic functionality of the connect and listen pair
// ///
// /// Connect and listen both write a limited amount of data and then EOF.
// /// The interaction should stop when both sides have EOF'd.
// #[test]
// fn connect_listen_happy() {
//     // the bytes provided by the listen command
//     let listen_to_connect = b"hello from listen";
//     let connect_to_listen = b"hello from connect";
//     let mut listen = duct::cmd(dumbpipe_bin(), ["listen"])
//         .env_remove("RUST_LOG") // disable tracing
//         .stdin_bytes(listen_to_connect)
//         .stderr_to_stdout() //
//         .reader()
//         .unwrap();
//     // read the first 3 lines of the header, and parse the last token as a ticket
//     let header = read_ascii_lines(3, &mut listen).unwrap();
//     let header = String::from_utf8(header).unwrap();
//     let ticket = header.split_ascii_whitespace().last().unwrap();
//     let ticket = NodeTicket::from_str(ticket).unwrap();

//     let connect = duct::cmd(dumbpipe_bin(), ["connect", &ticket.to_string()])
//         .env_remove("RUST_LOG") // disable tracing
//         .stdin_bytes(connect_to_listen)
//         .stderr_null()
//         .stdout_capture()
//         .run()
//         .unwrap();

//     assert!(connect.status.success());
//     assert_eq!(&connect.stdout, listen_to_connect);

//     let mut listen_stdout = Vec::new();
//     listen.read_to_end(&mut listen_stdout).unwrap();
//     assert_eq!(&listen_stdout, connect_to_listen);
// }

// #[cfg(unix)]
// #[test]
// fn connect_listen_ctrlc_connect() {
//     use nix::{
//         sys::signal::{self, Signal},
//         unistd::Pid,
//     };
//     // the bytes provided by the listen command
//     let mut listen = duct::cmd(dumbpipe_bin(), ["listen"])
//         .env_remove("RUST_LOG") // disable tracing
//         .stdin_bytes(b"hello from listen\n")
//         .stderr_to_stdout() //
//         .reader()
//         .unwrap();
//     // read the first 3 lines of the header, and parse the last token as a ticket
//     let header = read_ascii_lines(3, &mut listen).unwrap();
//     let header = String::from_utf8(header).unwrap();
//     let ticket = header.split_ascii_whitespace().last().unwrap();
//     let ticket = NodeTicket::from_str(ticket).unwrap();

//     let mut connect = duct::cmd(dumbpipe_bin(), ["connect", &ticket.to_string()])
//         .env_remove("RUST_LOG") // disable tracing
//         .stderr_null()
//         .stdout_capture()
//         .reader()
//         .unwrap();
//     // wait until we get a line from the listen process
//     read_ascii_lines(1, &mut connect).unwrap();
//     for pid in connect.pids() {
//         signal::kill(Pid::from_raw(pid as i32), Signal::SIGINT).unwrap();
//     }

//     let mut tmp = Vec::new();
//     // we don't care about the results. This test is just to make sure that the
//     // listen command stops when the connect command stops.
//     listen.read_to_end(&mut tmp).ok();
//     connect.read_to_end(&mut tmp).ok();
// }

// #[cfg(unix)]
// #[test]
// fn connect_listen_ctrlc_listen() {
//     use std::time::Duration;

//     use nix::{
//         sys::signal::{self, Signal},
//         unistd::Pid,
//     };
//     // the bytes provided by the listen command
//     let mut listen = duct::cmd(dumbpipe_bin(), ["listen"])
//         .env_remove("RUST_LOG") // disable tracing
//         .stderr_to_stdout()
//         .reader()
//         .unwrap();
//     // read the first 3 lines of the header, and parse the last token as a ticket
//     let header = read_ascii_lines(3, &mut listen).unwrap();
//     let header = String::from_utf8(header).unwrap();
//     let ticket = header.split_ascii_whitespace().last().unwrap();
//     let ticket = NodeTicket::from_str(ticket).unwrap();

//     let mut connect = duct::cmd(dumbpipe_bin(), ["connect", &ticket.to_string()])
//         .env_remove("RUST_LOG") // disable tracing
//         .stderr_null()
//         .stdout_capture()
//         .reader()
//         .unwrap();
//     std::thread::sleep(Duration::from_secs(1));
//     for pid in listen.pids() {
//         signal::kill(Pid::from_raw(pid as i32), Signal::SIGINT).unwrap();
//     }

//     let mut tmp = Vec::new();
//     // we don't care about the results. This test is just to make sure that the
//     // listen command stops when the connect command stops.
//     listen.read_to_end(&mut tmp).ok();
//     connect.read_to_end(&mut tmp).ok();
// }

// #[test]
// fn listen_tcp_happy() {
//     let b1 = wait2();
//     let b2 = b1.clone();
//     let port = random_port();
//     // start a dummy tcp server and wait for a single incoming connection
//     let host_port = format!("localhost:{port}");
//     let host_port_2 = host_port.clone();
//     std::thread::spawn(move || {
//         let server = TcpListener::bind(host_port_2).unwrap();
//         b1.wait();
//         let (mut stream, _addr) = server.accept().unwrap();
//         stream.write_all(b"hello from tcp").unwrap();
//         stream.flush().unwrap();
//         drop(stream);
//     });
//     // wait for the tcp listener to start
//     b2.wait();
//     // start a dumbpipe listen-tcp process
//     let mut listen_tcp = duct::cmd(dumbpipe_bin(), ["listen-tcp", "--host", &host_port])
//         .env_remove("RUST_LOG") // disable tracing
//         .stderr_to_stdout() //
//         .reader()
//         .unwrap();
//     let header = read_ascii_lines(4, &mut listen_tcp).unwrap();
//     let header = String::from_utf8(header).unwrap();
//     let ticket = header.split_ascii_whitespace().last().unwrap();
//     let ticket = NodeTicket::from_str(ticket).unwrap();
//     // poke the listen-tcp process with a connect command
//     let connect = duct::cmd(dumbpipe_bin(), ["connect", &ticket.to_string()])
//         .env_remove("RUST_LOG") // disable tracing
//         .stderr_null()
//         .stdout_capture()
//         .stdin_bytes(b"hello from connect")
//         .run()
//         .unwrap();
//     assert!(connect.status.success());
//     assert_eq!(&connect.stdout, b"hello from tcp");
// }

// #[test]
// fn connect_tcp_happy() {
//     let port = random_port();
//     let host_port = format!("localhost:{port}");
//     // start a dumbpipe listen process just so the connect-tcp command has something to connect to
//     let mut listen = duct::cmd(dumbpipe_bin(), ["listen"])
//         .env_remove("RUST_LOG") // disable tracing
//         .stdin_bytes(b"hello from listen\n")
//         .stderr_to_stdout() //
//         .reader()
//         .unwrap();
//     let header = read_ascii_lines(3, &mut listen).unwrap();
//     let header = String::from_utf8(header).unwrap();
//     let ticket = header.split_ascii_whitespace().last().unwrap();
//     let ticket = NodeTicket::from_str(ticket).unwrap();
//     let ticket = ticket.to_string();

//     // start a dumbpipe connect-tcp process
//     let _connect_tcp = duct::cmd(
//         dumbpipe_bin(),
//         ["connect-tcp", "--addr", &host_port, &ticket],
//     )
//     .env_remove("RUST_LOG") // disable tracing
//     .stderr_to_stdout() //
//     .reader()
//     .unwrap();
//     std::thread::sleep(Duration::from_secs(1));

//     //
//     let mut conn = TcpStream::connect(host_port).unwrap();
//     conn.write_all(b"hello from tcp").unwrap();
//     conn.flush().unwrap();
//     let mut buf = Vec::new();
//     conn.read_to_end(&mut buf).unwrap();
//     assert_eq!(&buf, b"hello from listen\n");
// }
