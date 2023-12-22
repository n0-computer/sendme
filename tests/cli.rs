use std::{
    io::{self, Read},
    path::{Path, PathBuf},
    str::FromStr,
};

use iroh_net::ticket::BlobTicket;

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
fn send_recv_dir() {
    fn create_file(base: &Path, i: usize, j: usize, k: usize) -> (PathBuf, Vec<u8>) {
        let name = base
            .join(format!("dir-{}", i))
            .join(format!("subdir-{}", j))
            .join(format!("file-{}", k));
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
