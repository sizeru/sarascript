use core::{
	fmt::{Display, Debug},
	marker::{Send, Sync},
	str::Utf8Error
};
use http::{Response, StatusCode, header};
use crate::server::{HttpBody, ContentType};

type ExternalError = Box<dyn std::error::Error + Send + Sync>;

pub fn is_directory(error: &std::io::Error) -> bool {
	match error.raw_os_error() {
		Some(os_error) => os_error == libc::EISDIR,
		None => false,
	}
}

pub enum SaraError {
	External(ExternalError),
	ChrootNotPermitted(String),
	FileInvalidPermissions(String),
	FileNotFound(String),
	OtherIOError(std::io::Error),
	HtmlFileNotUtf8(Utf8Error),
	FailedParsingSarascript(pest::error::Error<crate::Rule>),
	JoinError(Vec<tokio::task::JoinError>),
	HttpMethodUnsuported(http::Method),
	HttpHostInvalid(String),
	FailedToBuildResponse(http::Error),
	FailedToBuildRequest(http::Error),
	FailedToReadConfig(ExternalError),
	FailedToSetConfig(tokio::sync::SetError<crate::ConfigSettings>),
	ConfigError(config::ConfigError),
	DaemonizingFailed(daemonize::Error),
	FailedToWriteToStream(std::io::Error),
	FrameError(hyper::Error),
	FailedToSendRequest(hyper::Error),
	InvalidUri(http::uri::InvalidUri),
	DnsResolution(hickory_resolver::error::ResolveError),
}

impl SaraError {
	pub fn to_response(self) -> Response<HttpBody> {
		let plaintext = ContentType::Plain.as_str();
		let response: Result<Response<HttpBody>, http::Error> = match self {
			Self::FileInvalidPermissions(_) |  Self::FileNotFound(_) => Response::builder().status(StatusCode::NOT_FOUND).header(header::CONTENT_TYPE, plaintext).body("URL not available".into()),
			Self::OtherIOError(_) => Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).header(header::CONTENT_TYPE, plaintext).body("URL not available due to server error. Please check back later".into()),
			Self::HtmlFileNotUtf8(_) => Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).header(header::CONTENT_TYPE, plaintext).body("The URI requested is corrupted (invalid UTF-8)".into()),
			Self::ChrootNotPermitted(_) | Self::External(_) => Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).header(header::CONTENT_TYPE, plaintext).body("An unkown error occured".into()),
			Self::FailedParsingSarascript(_) => Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).header(header::CONTENT_TYPE, plaintext).body("Unable to parse the requested script".into()),
			Self::JoinError(_) => Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).header(header::CONTENT_TYPE, plaintext).body("Failed to parse one or more scripts on this page".into()),
			Self::HttpMethodUnsuported(method) => Response::builder().status(StatusCode::METHOD_NOT_ALLOWED).header(header::CONTENT_TYPE, plaintext).body(format!("HTTP method '{method}' is unsupported.").into()),
			Self::HttpHostInvalid(host) => Response::builder().status(StatusCode::BAD_REQUEST).header(header::CONTENT_TYPE, plaintext).body(format!("Server cannot respond to requests to host '{host}'").into()),
			Self::FailedToBuildResponse(_) => Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).header(header::CONTENT_TYPE, plaintext).body("Server encountered an unkown error".into()),
			Self::FailedToBuildRequest(_) => unreachable!(),
			Self::FailedToReadConfig(_) => unreachable!(),
			Self::FailedToSetConfig(_) => unreachable!(),
			Self::ConfigError(_) => unreachable!(),
			Self::FailedToWriteToStream(_) => unreachable!(),
			Self::DaemonizingFailed(_) => unreachable!(),
			Self::FrameError(_) => unreachable!(),
			Self::FailedToSendRequest(_) => unreachable!(),
			Self::InvalidUri(_) => Response::builder().status(StatusCode::BAD_REQUEST).header(header::CONTENT_TYPE, ContentType::Plain.as_str()).body("Invalid uri".into()),
			Self::DnsResolution(_) => Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).header(header::CONTENT_TYPE, ContentType::Plain.as_str()).body("Failed to resolve DNS of host".into()),
		};
		return response.unwrap();
	}
}

impl Debug for SaraError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{self}")
	}
}

impl Display for SaraError {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		match self {
			Self::External(source) => write!(f, "{source}"),
			Self::ChrootNotPermitted(path) => write!(f, "not permitted to change root to `{path}` (may occur if not running as root)"),
			Self::FileInvalidPermissions(uri) => write!(f, "uri `{uri} requested, but permissions were invalid"),
			Self::FileNotFound(uri) => write!(f, "no file or api found at {uri}"),
			Self::HtmlFileNotUtf8(source) => write!(f, "{source}"),
			Self::FailedParsingSarascript(source) => write!(f, "{source}"),
			Self::OtherIOError(source) => write!(f, "{source}"),
			Self::JoinError(join_errors) => write!(f, "not able to join {} handle(s): {join_errors:?}", join_errors.len()),
			Self::HttpMethodUnsuported(method) => write!(f, "method '{method}' is unsupported"),
			Self::HttpHostInvalid(host) => write!(f, "not sure how to handle host '{host}'"),
			Self::FailedToBuildResponse(source) => write!(f, "{source}"),
			Self::FailedToBuildRequest(source) => write!(f, "{source}"),
			Self::FailedToReadConfig(source) => write!(f, "failed to read config: {source}"),
			Self::FailedToSetConfig(source) => write!(f, "failed to set config: {source}"),
			Self::ConfigError(source) => write!(f, "config error: {source}"),
			Self::FailedToWriteToStream(source) => write!(f, "failed to write to sream: {source}"),
			Self::DaemonizingFailed(source) => write!(f, "could not daemonize: {source}"),
			Self::FrameError(source) => write!(f, "could not retrieve http frame: {source}"),
			Self::FailedToSendRequest(source) => write!(f, "failed to send request: {source}"),
			Self::InvalidUri(source) => write!(f, "invalid uri: {source}"),
			Self::DnsResolution(source) => write!(f, "dns resolution failed: {source}"),
		}
	}
}

// Implementation for the config crate
impl From<config::ConfigError> for SaraError {
	fn from(value: config::ConfigError) -> Self {
		Self::ConfigError(value)
	}
}

impl From<http::uri::InvalidUri> for SaraError {
	fn from(value: http::uri::InvalidUri) -> Self {
		Self::InvalidUri(value)
	}
}

impl From<std::io::Error> for SaraError {
	fn from(io_error: std::io::Error) -> Self {
		Self::OtherIOError(io_error)
	}
}