use core::fmt;

#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ContentHash([u8; blake3::OUT_LEN]);

impl ContentHash {
    #[must_use]
    pub fn of(content: &[u8]) -> Self {
        Self(*blake3::hash(content).as_bytes())
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; blake3::OUT_LEN] {
        &self.0
    }

    #[must_use]
    pub const fn from_bytes(bytes: [u8; blake3::OUT_LEN]) -> Self {
        Self(bytes)
    }
}

impl fmt::Debug for ContentHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "ContentHash({self})")
    }
}

impl fmt::Display for ContentHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}
