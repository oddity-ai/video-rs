/// Re-export [`url::Url`] since it is an input type for callers of the API.
pub use url::Url;

/// TODO
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
    pub fn as_path(&self) -> &std::path::Path {
        match self {
            Location::File { path } => path.as_path(),
            Location::Network { url } => &std::path::Path::new(url.as_str()),
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

/// TODO
#[derive(Debug, PartialEq, Eq, Hash)]
pub enum LocationRef<'a> {
    File {
        /// Path to underlying file.
        path: &'a std::path::Path,
    },
    Network {
        /// URL pointing to network stream.
        url: &'a Url,
    },
}

impl<'a> LocationRef<'a> {
    pub fn as_path(&self) -> &std::path::Path {
        match self {
            LocationRef::File { path } => path,
            LocationRef::Network { url } => &std::path::Path::new(url.as_str()),
        }
    }
}

impl<'a> std::borrow::Borrow<LocationRef<'a>> for Location {
    fn borrow(&self) -> &'a LocationRef<'a> {
        match self {
            Location::File { path } => &LocationRef::File { path: &path },
            Location::Network { url } => &LocationRef::Network { url: &url },
        }
    }
}

impl<'a> AsRef<LocationRef<'a>> for std::path::PathBuf {
    fn as_ref(&self) -> &'a LocationRef<'a> {
        &LocationRef::File {
            path: self.as_path(),
        }
    }
}

impl<'a> AsRef<LocationRef<'a>> for std::path::Path {
    fn as_ref(&self) -> &'a LocationRef<'a> {
        &LocationRef::File { path: self }
    }
}

impl<'a> AsRef<LocationRef<'a>> for Url {
    fn as_ref(&self) -> &'a LocationRef<'a> {
        &LocationRef::Network { url: &self }
    }
}

impl<'a> ToOwned for LocationRef<'a> {
    type Owned = Location;

    fn to_owned(&self) -> Self::Owned {
        match self {
            LocationRef::File { path } => Location::File {
                path: path.to_path_buf(),
            },
            LocationRef::Network { url } => Location::Network { url: *url.clone() },
        }
    }
}

impl<'a> std::fmt::Display for LocationRef<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LocationRef::File { path } => write!(f, "{}", path.display()),
            LocationRef::Network { url } => write!(f, "{url}"),
        }
    }
}
