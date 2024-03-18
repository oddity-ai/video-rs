/// Re-export [`url::Url`] since it is an input type for callers of the API.
pub use url::Url;

/// Represents a video file or stream location. Can be either a file resource (a path) or a network
/// resource (a URL).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Location {
    File {
        /// Path to underlying file.
        path: std::path::PathBuf,
    },
    Network {
        /// URL pointing to network stream.
        url: Url,
    },
}

impl Location {
    /// Coerce underlying location to a path.
    ///
    /// This will create a path with a URL in it (which is kind of weird but we use it to pass on
    /// URLs to ffmpeg).
    pub fn as_path(&self) -> &std::path::Path {
        match self {
            Location::File { path } => path.as_path(),
            Location::Network { url } => std::path::Path::new(url.as_str()),
        }
    }
}

impl From<std::path::PathBuf> for Location {
    fn from(value: std::path::PathBuf) -> Location {
        Location::File { path: value }
    }
}

impl From<&std::path::Path> for Location {
    fn from(value: &std::path::Path) -> Location {
        Location::File {
            path: value.to_path_buf(),
        }
    }
}

impl From<Url> for Location {
    fn from(value: Url) -> Location {
        Location::Network { url: value }
    }
}

impl From<&Url> for Location {
    fn from(value: &Url) -> Location {
        Location::Network { url: value.clone() }
    }
}

impl std::fmt::Display for Location {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Location::File { path } => write!(f, "{}", path.display()),
            Location::Network { url } => write!(f, "{url}"),
        }
    }
}
