use std::fmt;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub enum Error {

    NotAFolderName { name: String },
    NoSuchLibrary { id: i64 },

    AlreadyExists { path: PathBuf },
    NotAFolder { path: PathBuf },

    AlreadyInCatalog { path: PathBuf },

    NotConnected { root: PathBuf },

    EditedInNewerVersion,
    NeedsAName { of: Named },
    AlbumExists { name: String },

    Other(String),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Named {
    Album,
    Snapshot,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotAFolderName { name } => write!(f, "“{name}” cannot be a folder name"),
            Error::NoSuchLibrary { id } => write!(f, "no library {id}"),
            Error::AlreadyExists { path } => write!(f, "{} already exists", path.display()),
            Error::NotAFolder { path } => write!(f, "{} is not a folder", path.display()),
            Error::AlreadyInCatalog { path } => write!(f, "{} is already in the catalog", path.display()),
            Error::NotConnected { root } => write!(f, "{} is not connected", root.display()),
            Error::EditedInNewerVersion => f.write_str("This photograph was edited in a newer version of Numa — update Numa to change it here"),
            Error::NeedsAName { of: Named::Album } => f.write_str("An album needs a name"),
            Error::NeedsAName { of: Named::Snapshot } => f.write_str("A snapshot needs a name"),
            Error::AlbumExists { name } => write!(f, "There is already an album called “{name}”"),
            Error::Other(text) => f.write_str(text),
        }
    }
}

impl std::error::Error for Error {}

impl From<String> for Error {
    fn from(text: String) -> Self {
        Error::Other(text)
    }
}

impl From<Error> for String {
    fn from(error: Error) -> Self {
        error.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_english_is_unchanged() {
        assert_eq!(Error::NotAFolderName { name: "a/b".into() }.to_string(), "“a/b” cannot be a folder name");
        assert_eq!(Error::NoSuchLibrary { id: 3 }.to_string(), "no library 3");
        assert_eq!(Error::AlreadyExists { path: "/p/x".into() }.to_string(), "/p/x already exists");
        assert_eq!(Error::NotAFolder { path: "/p/x".into() }.to_string(), "/p/x is not a folder");
        assert_eq!(Error::AlreadyInCatalog { path: "/p/x".into() }.to_string(), "/p/x is already in the catalog");
        assert_eq!(Error::NotConnected { root: "/d".into() }.to_string(), "/d is not connected");
        assert_eq!(Error::NeedsAName { of: Named::Album }.to_string(), "An album needs a name");
        assert_eq!(Error::NeedsAName { of: Named::Snapshot }.to_string(), "A snapshot needs a name");
        assert_eq!(Error::AlbumExists { name: "x".into() }.to_string(), "There is already an album called “x”");
        assert_eq!(String::from(Error::from("anything".to_string())), "anything");
    }
}
