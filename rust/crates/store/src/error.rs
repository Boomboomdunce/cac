use serde_json;
use std::{fmt, io};

#[derive(Debug)]
pub enum StoreError {
    Io(io::Error),
    Serde(serde_json::Error),
    ProfileNotFound(String),
    InvalidProfileName(String),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::Io(err) => write!(f, "I/O error: {err}").map(|_| ()),
            StoreError::Serde(err) => write!(f, "serialization error: {err}").map(|_| ()),
            StoreError::ProfileNotFound(name) => write!(f, "profile `{name}` not found"),
            StoreError::InvalidProfileName(name) => write!(f, "invalid profile name `{name}`"),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<io::Error> for StoreError {
    fn from(err: io::Error) -> Self {
        StoreError::Io(err)
    }
}

impl From<serde_json::Error> for StoreError {
    fn from(err: serde_json::Error) -> Self {
        StoreError::Serde(err)
    }
}
