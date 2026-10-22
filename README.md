# Sendme

[![Crates.io](https://img.shields.io/crates/v/sendme.svg?style=flat-square)](https://crates.io/crates/sendme)
[![downloads](https://img.shields.io/crates/d/sendme.svg?style=flat-square)](https://crates.io/crates/sendme)
[![Chat](https://img.shields.io/discord/1161119546170687619?logo=discord&style=flat-square)](https://discord.com/invite/DpmJgtU7cW)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg?style=flat-square)](LICENSE-MIT)
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg?style=flat-square)](LICENSE-APACHE)
[![CI](https://img.shields.io/github/actions/workflow/status/n0-computer/sendme/ci.yml?branch=main&style=flat-square&label=CI)](https://github.com/n0-computer/sendme/actions/workflows/ci.yml)


This is an example application using [iroh](https://crates.io/crates/iroh) with
the [iroh-blobs](https://crates.io/crates/iroh-blobs) protocol to send files and
directories over the internet.

It is also useful as a standalone tool for quick copy jobs.

Iroh will take care of hole punching and NAT traversal whenever possible,
and fall back to a relay if hole punching does not succeed.

Iroh-blobs will take care of [blake3](https://crates.io/crates/blake3) verified
streaming, including resuming interrupted downloads.

Sendme works with 256 bit node ids and is, therefore, location transparent. A ticket
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

The provider will run until it is terminated using `Control-C`. On termination, it
will delete the temporary directory.

This currently will create a temporary directory in the current directory. In
the future this won't be needed anymore.

### Ignore files

By default, sendme respects ignore files when sending a directory. To include
ignored files and `.git` metadata, use `--no-ignore`:

```sh
sendme send ./project             # Respect ignore files
sendme send ./project --no-ignore # Send every regular file
```

Default filtering reads `.gitignore` and `.ignore` files within the selected
directory and its descendants. No Git repository is required. `.ignore` rules
override `.gitignore` rules; within each kind, rules closer to a file take
precedence. Rules cannot rescue a file in an excluded directory.

Default filtering also omits `.git` files and directories. Other hidden files and
the ignore files themselves are sent unless excluded. An explicitly selected file
is always sent, even if it would be ignored during directory traversal.

Parent-directory rules, global Git configuration, `.git/info/exclude`, and
`.dockerignore` files are not rule sources. This is file filtering, not an export
of Git-tracked files. Missing ignore files are allowed; when filtering is enabled,
invalid, unreadable, or non-regular ignore files cause an error, as does filtering
out every file. `--no-ignore` skips reading ignore files entirely. Descendant
symlinks are not followed, with or without `--no-ignore`.

### Receive side

```
sendme receive <ticket>
```

This will download the data and create a file or directory named like the source
in the **current directory**.

It will create a temporary directory in the current directory, download the data
(single file or directory), and only then move these files to the target
directory.

On completion, it will delete the temp directory.

All temp directories start with `.sendme-`.

## License

Copyright 2026 N0, INC.

This project is licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or
   <http://www.apache.org/licenses/LICENSE-2.0>)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or
   <http://opensource.org/licenses/MIT>)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this project by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
