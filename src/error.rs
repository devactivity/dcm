use std::fmt;

#[derive(Debug)]
pub enum Error {
    Api(String),
    Io(std::io::Error),
    Http(reqwest::Error),
    Json(serde_json::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Api(msg) => write!(f, "docker API error: {msg}"),
            Error::Io(err) => write!(f, "IO error: {err}"),
            Error::Http(err) => write!(f, "HTTP error: {err}"),
            Error::Json(err) => write!(f, "JSON error: {err}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error::Io(err)
    }
}

impl From<reqwest::Error> for Error {
    fn from(err: reqwest::Error) -> Self {
        Error::Http(err)
    }
}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Error::Json(err)
    }
}
