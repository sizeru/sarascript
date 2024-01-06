use core::fmt::{Display, Debug};
use core::marker::{Send, Sync};

#[derive(Debug)]
pub enum SaraError {
	External(Box<dyn std::error::Error + Send + Sync>),
}

impl Display for SaraError {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		match self {
			Self::External(source) => write!(f, "{source}"),
		}
	}
}

// Implementation for the config crate
impl From<config::ConfigError> for SaraError {
	fn from(value: config::ConfigError) -> Self {
		Self::External(Box::new(value))
	}
}

// Implementation for erors from tokio::sync::OnceCell
impl From<tokio::sync::SetError<crate::ConfigSettings>> for SaraError {
	fn from(value: tokio::sync::SetError<crate::ConfigSettings>) -> Self {
		Self::External(Box::new(value))
	}
}

impl From<tokio::task::JoinError> for SaraError {
	fn from(value: tokio::task::JoinError) -> Self {
		Self::External(Box::new(value))
	}
}

impl From<tokio::io::Error> for SaraError {
	fn from(value: tokio::io::Error) -> Self {
		Self::External(Box::new(value))
	}
}

impl From<hyper::Error> for SaraError {
	fn from(value: hyper::Error) -> Self {
		Self::External(Box::new(value))
	}
}

impl From<core::str::Utf8Error> for SaraError {
	fn from(value: core::str::Utf8Error) -> Self {
		Self::External(Box::new(value))
	}
}

impl<T> From<pest::error::Error<T>> for SaraError 
where T: Debug + Send + Sync + Copy + core::hash::Hash + Ord {
	fn from(value: pest::error::Error<T>) -> Self {
		Self::External(Box::new(value))
	}
}

impl From<http::uri::InvalidUri> for SaraError {
	fn from(value: http::uri::InvalidUri) -> Self {
		Self::External(Box::new(value))
	}
}

impl From<http::Error> for SaraError {
	fn from(value: http::Error) -> Self {
		Self::External(Box::new(value))
	}
}