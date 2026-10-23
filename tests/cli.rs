use std::{
    io::{self, Read},
    path::{Path, PathBuf},
    str::FromStr,
};

use iroh_blobs::ticket::BlobTicket;

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

// fn wait2() -> Arc<Barrier> {
//     Arc::new(Barrier::new(2))
// }

// /// generate a random, non privileged port
// fn random_port() -> u16 {
//     rand::thread_rng().gen_range(10000u16..60000)
// }

#[test]
fn send_recv_file() {
    let name = "somefile.bin";
    let data = vec![0u8; 100];
    // create src and tgt dir, and src file
    let src_dir = tempfile::tempdir().unwrap();
    let tgt_dir = tempfile::tempdir().unwrap();
    let src_file = src_dir.path().join(name);
    std::fs::write(&src_file, &data).unwrap();
    let mut send_cmd = duct::cmd(
        sendme_bin(),
        ["send", src_file.as_os_str().to_str().unwrap()],
    )
    .dir(src_dir.path())
    .env_remove("RUST_LOG") // disable tracing
    .stderr_to_stdout()
    .reader()
    .unwrap();
    let output = read_ascii_lines(3, &mut send_cmd).unwrap();
    let output = String::from_utf8(output).unwrap();
    let ticket = output.split_ascii_whitespace().last().unwrap();
    let ticket = BlobTicket::from_str(ticket).unwrap();
    let receive_output = duct::cmd(sendme_bin(), ["receive", &ticket.to_string()])
        .dir(tgt_dir.path())
        .env_remove("RUST_LOG") // disable tracing
        .stderr_to_stdout()
        .run()
        .unwrap();
    assert!(receive_output.status.success());
    let tgt_file = tgt_dir.path().join(name);
    let tgt_data = std::fs::read(tgt_file).unwrap();
    assert_eq!(tgt_data, data);
}

#[test]
fn receive_closes_endpoint_no_iroh_socket_error() {
    let name = "graceful-close.bin";
    let data = vec![0xabu8; 64];
    let src_dir = tempfile::tempdir().unwrap();
    let tgt_dir = tempfile::tempdir().unwrap();
    let src_file = src_dir.path().join(name);
    std::fs::write(&src_file, &data).unwrap();
    let mut send_cmd = duct::cmd(
        sendme_bin(),
        ["send", src_file.as_os_str().to_str().unwrap()],
    )
    .dir(src_dir.path())
    .env_remove("RUST_LOG")
    .stderr_to_stdout()
    .reader()
    .unwrap();
    let output = read_ascii_lines(3, &mut send_cmd).unwrap();
    let output = String::from_utf8(output).unwrap();
    let ticket = output.split_ascii_whitespace().last().unwrap();
    let ticket = BlobTicket::from_str(ticket).unwrap();
    let receive_output = duct::cmd(sendme_bin(), ["receive", &ticket.to_string()])
        .dir(tgt_dir.path())
        .env("RUST_LOG", "iroh::socket=error")
        .stdout_capture()
        .stderr_capture()
        .run()
        .unwrap();
    assert!(receive_output.status.success(), "{receive_output:?}");
    let stderr = String::from_utf8_lossy(&receive_output.stderr);
    assert!(
        !stderr.contains("Endpoint dropped"),
        "unexpected iroh shutdown log on stderr: {stderr}"
    );
    assert!(
        !stderr.contains("Aborting ungracefully"),
        "unexpected iroh shutdown log on stderr: {stderr}"
    );
    let tgt_file = tgt_dir.path().join(name);
    assert_eq!(std::fs::read(&tgt_file).unwrap(), data);
}

#[test]
fn send_recv_dir() {
    fn create_file(base: &Path, i: usize, j: usize, k: usize) -> (PathBuf, Vec<u8>) {
        let name = base
            .join(format!("dir-{i}"))
            .join(format!("subdir-{j}"))
            .join(format!("file-{k}"));
        let len = i * 100 + j * 10 + k;
        let data = vec![0u8; len];
        (name, data)
    }

    // create src and tgt dir, and src file
    let src_dir = tempfile::tempdir().unwrap();
    let tgt_dir = tempfile::tempdir().unwrap();
    let src_data_dir = src_dir.path().join("data");
    let tgt_data_dir = tgt_dir.path().join("data");
    // create a complex directory structure
    for i in 0..5 {
        for j in 0..5 {
            for k in 0..5 {
                let (name, data) = create_file(&src_data_dir, i, j, k);
                std::fs::create_dir_all(name.parent().unwrap()).unwrap();
                std::fs::write(&name, &data).unwrap();
            }
        }
    }
    let mut send_cmd = duct::cmd(
        sendme_bin(),
        ["send", src_data_dir.as_os_str().to_str().unwrap()],
    )
    .dir(src_dir.path())
    .env_remove("RUST_LOG") // disable tracing
    .stderr_to_stdout()
    .reader()
    .unwrap();
    let output = read_ascii_lines(3, &mut send_cmd).unwrap();
    let output = String::from_utf8(output).unwrap();
    let ticket = output.split_ascii_whitespace().last().unwrap();
    let ticket = BlobTicket::from_str(ticket).unwrap();
    let receive_output = duct::cmd(sendme_bin(), ["receive", &ticket.to_string()])
        .dir(tgt_dir.path())
        .env_remove("RUST_LOG") // disable tracing
        .stderr_to_stdout()
        .run()
        .unwrap();
    assert!(receive_output.status.success());
    // validate directory structure
    for i in 0..5 {
        for j in 0..5 {
            for k in 0..5 {
                let (name, data) = create_file(&tgt_data_dir, i, j, k);
                let tgt_data = std::fs::read(&name).unwrap();
                assert_eq!(tgt_data, data);
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn send_reports_completed_downloads() {
    use std::{
        io::{BufRead as _, BufReader, Read as _},
        process::{Command, Stdio},
    };

    let name = "counted.bin";
    let data = vec![0u8; 100];
    let src_dir = tempfile::tempdir().unwrap();
    let tgt_dir = tempfile::tempdir().unwrap();
    let src_file = src_dir.path().join(name);
    std::fs::write(&src_file, &data).unwrap();
    let mut send_cmd = Command::new(sendme_bin())
        .args(["send", src_file.as_os_str().to_str().unwrap()])
        .current_dir(src_dir.path())
        .env_remove("RUST_LOG") // disable tracing
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdout = BufReader::new(send_cmd.stdout.take().unwrap());
    let mut header = String::new();
    for _ in 0..3 {
        header.clear();
        stdout.read_line(&mut header).unwrap();
    }
    let ticket = header.split_ascii_whitespace().last().unwrap();
    let receive_output = duct::cmd(sendme_bin(), ["receive", ticket])
        .dir(tgt_dir.path())
        .env_remove("RUST_LOG") // disable tracing
        .stderr_to_stdout()
        .run()
        .unwrap();
    assert!(receive_output.status.success());
    // stop the sender gracefully (ctrl-c) so it prints its summary
    let status = Command::new("kill")
        .args(["-INT", &send_cmd.id().to_string()])
        .status()
        .unwrap();
    assert!(status.success());
    let mut output = String::new();
    stdout.read_to_string(&mut output).unwrap();
    assert!(send_cmd.wait().unwrap().success());
    // one `receive` issues two requests against the provider (hash seq + blob),
    // so the sender should report two completed downloads
    assert!(
        output.contains("2 completed downloads"),
        "sender never reported the completed downloads: {output}"
    );
}
