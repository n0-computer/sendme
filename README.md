# Sendme

This is an example to use [iroh-bytes](https://crates.io/crates/iroh-bytes) and
[iroh-net](https://crates.io/crates/iroh-net) to send files over the internet.

Iroh-net will take case of hole punching and NAT traversal whenever possible, and
fall back to a relay if hole punching does not succeed.

Iroh-bytes will take care of blake3 verified streaming, including resuming
interrupted downloads.

It is also useful as a standalone tool for quick copy jobs.

Sendme works with 256 bit node ids and therefore is somewhat location transparent. In addition, connections are encrypted using TLS.

# Installation

```
cargo install sendme
```

# Usage

## Send side

```
sendme provide <file or directory>
```

This will create a temporary iroh node that serves the content in the given file or directory.
It will output a ticket that can be used to get the data.

This currently will create a temporary directory in the current directory. In the future this
won't be needed anymore.

The provider will run until it is terminated using Control-C. On termination it will delete
the temporary directory.

### Receive side

```
sendme get <ticket>
```

This will download the data and create a file or directory named like the source
in the **current directory**.

It will create a temporary directory in the current directory, download the data (single
file or files), and only then move these files to the target directory.

On completion it will delete the temp directory.
