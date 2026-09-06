use core::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error {
    InvalidPath,
    InvalidChunkSize,
    ContentTooLarge,
    NotFound,
    ContentUnavailable,
    CorruptContent,
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPath => formatter.write_str("path must name a file within a folder"),
            Self::InvalidChunkSize => formatter.write_str("chunk size must not be zero"),
            Self::ContentTooLarge => {
                formatter.write_str("content exceeds the maximum supported size")
            }
            Self::NotFound => formatter.write_str("file entry was not found"),
            Self::ContentUnavailable => formatter.write_str("file content is not stored locally"),
            Self::CorruptContent => formatter.write_str("stored content does not match its hash"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for Error {}
