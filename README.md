# Sendme

This is an example to use [iroh-bytes](https://crates.io/crates/iroh-bytes) and
[iroh-net](https://crates.io/crates/iroh-net) to send files and directories over
the internet.

It is also useful as a standalone tool for quick copy jobs.

Iroh-net will take case of hole punching and NAT traversal whenever possible,
and fall back to a relay if hole punching does not succeed.

Iroh-bytes will take care of [blake3](https://crates.io/crates/blake3) verified
streaming, including resuming interrupted downloads.

Sendme works with 256 bit node ids and therefore location transparent. A ticket
will remain valid if the IP address changes. Connections are encrypted using
TLS.

# Installation

```
cargo install sendme
```

# Usage

## Send side

```
sendme send <file or directory>
```

This will create a temporary [iroh](https://crates.io/crates/iroh) node that
serves the content in the given file or directory. It will output a ticket that
can be used to get the data.

The provider will run until it is terminated using Control-C. On termination it
will delete the temporary directory.

This currently will create a temporary directory in the current directory. In
the future this won't be needed anymore.

### Receive side

```
sendme receive <ticket>
```

This will download the data and create a file or directory named like the source
in the **current directory**.

It will create a temporary directory in the current directory, download the data
(single file or directory), and only then move these files to the target
directory.

On completion it will delete the temp directory.

All temp directories start with `.sendme-`.