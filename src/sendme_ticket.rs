//! Tickets for blobs.
//!
//! Copied over from iroh. It should live in iroh-bytes
use std::{fmt::Display, str::FromStr};

use anyhow::{ensure, Context, Result};
use iroh_bytes::{BlobFormat, Hash};
use iroh_net::NodeAddr;
use serde::{Deserialize, Serialize};

/// A token containing everything to get a file from the provider.
///
/// It is a single item which can be easily serialized and deserialized.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ticket {
    /// The provider to get a file from.
    node: NodeAddr,
    /// The format of the blob.
    format: BlobFormat,
    /// The hash to retrieve.
    hash: Hash,
}

impl Display for Ticket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.serialize())
    }
}

impl FromStr for Ticket {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::deserialize(s)
    }
}

#[allow(dead_code)]
impl Ticket {
    /// Creates a new ticket.
    pub fn new(node: NodeAddr, hash: Hash, format: BlobFormat) -> Result<Self> {
        ensure!(!node.info.is_empty(), "addressing info cannot be empty");
        Ok(Self { hash, format, node })
    }

    /// The hash of the item this ticket can retrieve.
    pub fn hash(&self) -> Hash {
        self.hash
    }

    /// The [`NodeAddr`] of the provider for this ticket.
    pub fn node_addr(&self) -> &NodeAddr {
        &self.node
    }

    /// The [`BlobFormat`] for this ticket.
    pub fn format(&self) -> BlobFormat {
        self.format
    }

    /// True if the ticket is for a collection and should retrieve all blobs in it.
    pub fn recursive(&self) -> bool {
        self.format.is_hash_seq()
    }

    /// Get the contents of the ticket, consuming it.
    pub fn into_parts(self) -> (NodeAddr, Hash, BlobFormat) {
        let Ticket { node, hash, format } = self;
        (node, hash, format)
    }

    /// Serialize to postcard bytes.
    fn to_bytes(&self) -> Vec<u8> {
        postcard::to_stdvec(&self).expect("postcard::to_stdvec is infallible")
    }

    /// Verify this ticket.
    fn verify(&self) -> anyhow::Result<()> {
        // do we need this? a ticket with just a node id still might be useful
        // given some sort of discovery mechanism.
        anyhow::ensure!(!self.node.info.is_empty(), "no node info");
        Ok(())
    }

    /// Deserialize from postcard bytes.
    fn from_bytes(bytes: &[u8]) -> anyhow::Result<Self> {
        let ticket: Self = postcard::from_bytes(bytes)?;
        ticket.verify().context("invalid ticket")?;
        Ok(ticket)
    }

    /// Serialize to string.
    fn serialize(&self) -> String {
        let mut out = "blob".to_string();
        data_encoding::BASE32_NOPAD.encode_append(&self.to_bytes(), &mut out);
        out.make_ascii_lowercase();
        out
    }

    /// Deserialize from a string.
    fn deserialize(str: &str) -> anyhow::Result<Self> {
        let Some(base32) = str.strip_prefix("blob") else {
            anyhow::bail!("invalid prefix");
        };
        let bytes = data_encoding::BASE32_NOPAD
            .decode(base32.to_ascii_uppercase().as_bytes())
            .context("invalid base32")?;
        Self::from_bytes(&bytes)
    }
}

impl Serialize for Ticket {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if serializer.is_human_readable() {
            serializer.serialize_str(&self.to_string())
        } else {
            let Ticket { node, format, hash } = self;
            (node, format, hash).serialize(serializer)
        }
    }
}

impl<'de> Deserialize<'de> for Ticket {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        if deserializer.is_human_readable() {
            let s = String::deserialize(deserializer)?;
            Self::from_str(&s).map_err(serde::de::Error::custom)
        } else {
            let (peer, format, hash) = Deserialize::deserialize(deserializer)?;
            Self::new(peer, hash, format).map_err(serde::de::Error::custom)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use bao_tree::blake3;
    use iroh_bytes::store::bao_tree;
    use iroh_net::key::SecretKey;

    use super::*;

    fn make_ticket() -> Ticket {
        let hash = blake3::hash(b"hi there");
        let hash = Hash::from(hash);
        let peer = SecretKey::generate().public();
        let addr = SocketAddr::from_str("127.0.0.1:1234").unwrap();
        let derp_url = None;
        Ticket {
            hash,
            node: NodeAddr::from_parts(peer, derp_url, vec![addr]),
            format: BlobFormat::HashSeq,
        }
    }

    #[test]
    fn test_ticket_postcard() {
        let ticket = make_ticket();
        let bytes = postcard::to_stdvec(&ticket).unwrap();
        let ticket2: Ticket = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(ticket2, ticket);
    }

    #[test]
    fn test_ticket_json() {
        let ticket = make_ticket();
        let json = serde_json::to_string(&ticket).unwrap();
        let ticket2: Ticket = serde_json::from_str(&json).unwrap();
        assert_eq!(ticket2, ticket);
    }

    #[test]
    fn test_ticket_base32_roundtrip() {
        let ticket = make_ticket();
        let base32 = ticket.to_string();
        println!("Ticket: {base32}");
        println!("{} bytes", base32.len());

        let ticket2: Ticket = base32.parse().unwrap();
        assert_eq!(ticket2, ticket);
    }
}
