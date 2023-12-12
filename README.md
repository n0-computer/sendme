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
