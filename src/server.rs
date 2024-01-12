use log::{info, warn, error, debug};
use daemonize::Daemonize;
use core::{
	convert::Infallible,
	pin::Pin,
	task::{Poll, Context},
	fmt::Display,
};
use hyper::{
	server::conn::http1,
	service::service_fn,
};
use std::{
	net::SocketAddr,
	process::ExitCode
};
use bytes::Bytes;
use http::{Method, Request, Response, header, StatusCode};
use http_body::{Body, Frame, SizeHint};
use tokio::{ net::TcpListener, fs, io};
use crate::{ hyper_tokio_adapter::HyperStream, parse_text, ConfigSettings, CONFIG_PATH, error::{SaraError, is_directory}, };

type Result<T, E=SaraError> = std::result::Result<T, E>;

struct File {
	contents: Vec<u8>,
	content_type: ContentType
}

impl File {
	pub fn new(contents: Vec<u8>, filename: &str) -> File {
		Self {
			contents,
			content_type: ContentType::from(filename)
		}
	}

	pub fn may_contain_scripts(&self) -> bool {
		self.content_type.may_contain_scripts()
	}
}

pub enum ContentType {
	Plain,
	Html,
	Css,
	Markdown,
	Pdf,
	Binary,
	Unknown,
	Svg,
}

impl Display for ContentType {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.as_str())
	}
}

impl From<&str> for ContentType {
	fn from(filename: &str) -> Self {
		let extension = filename.split('.').last();
		match extension {
			Some(extension) => match extension {
				"html" => Self::Html,
				"pdf" => Self::Pdf,
				"txt" => Self::Plain,
				"svg" => Self::Svg,
				"css" => Self::Css,
				_ => Self::Unknown,
			}
			None => Self::Binary,
		}
	}
}

impl ContentType {
	pub fn may_contain_scripts(&self) -> bool {
		match self {
			Self::Html | Self::Markdown => true,
			_ => false,
		}
	}

	pub fn as_str(&self) -> &'static str {
		match self {
			Self::Plain => "text/plain",
			Self::Html => "text/html",
			Self::Css => "text/css",
			Self::Markdown => "text/markdown",
			Self::Pdf => "application/pdf",
			Self::Svg => "image/svg+xml",
			Self::Binary => "application/octet-stream",
			Self::Unknown => "application/octet-stream",
		}
	}
}

// Run server without creating a daemon (as logged in user). Useful for debugging.
pub fn run_server() -> Result<ExitCode> {
	let config = ConfigSettings::init(CONFIG_PATH)?;
	std::os::unix::fs::chroot(&config.root).map_err(|error| {
		let kind = error.kind();
		match kind {
			io::ErrorKind::PermissionDenied => SaraError::ChrootNotPermitted(config.root.clone()),
			_ => SaraError::OtherIOError(error),
		}
	})?;
	let exit_code = run();
	return Ok(exit_code.into());
}

// Create a deamon and launch the server. This shouldbe used in production.
pub fn launch_server() -> Result<ExitCode> {
	let config = ConfigSettings::init(CONFIG_PATH)?;
	create_daemon(&config).start().map_err(|daemon_error| SaraError::DaemonizingFailed(daemon_error))?;
	let exit_code = run();
	return Ok(exit_code.into());
}

fn create_daemon(config: &ConfigSettings) -> Daemonize<()> {
	let log_filename = config.log_filename.clone();
	let daemon = Daemonize::new()
		.chroot(&config.root)
		.user(config.user.as_ref())
		.pid_file(&config.pid_filename)
		.chown_pid_file(true)
		.privileged_action(move || {
			let log = std::fs::OpenOptions::new()
				.append(true)
				.create(true)
				.open(log_filename)
				.expect(&format!("Could not access log file at here"));
			simplelog::WriteLogger::init(
				simplelog::LevelFilter::Info,
				simplelog::Config::default(),
				log,
			).expect("Could not initialize logger");
		});
	return daemon;
}

#[tokio::main]
async fn run() -> u8 {
	let config = unsafe { ConfigSettings::get_unchecked() };
	let addr: SocketAddr = config.bind_to.parse().expect("bind address is not correct");
	let listener = TcpListener::bind(addr).await.expect("Could not bind TCP Listener");
	// TODO: Would a new struct which implements Tcp Listening be useful?

	info!("Starting server on: {addr}");

	loop {
		let stream = match listener.accept().await {
			Ok((stream, _socket)) => stream,
			Err(e) => {
				warn!("Could not accept stream from listener: {e}");
				continue;
			},
		};

		let stream = HyperStream::Plain(stream);

		// Spawn a tokio task to serve multiple connections concurrently
		tokio::task::spawn(async move {
			// Finally, we bind the incoming connection to our `hello` service
			if let Err(err) = http1::Builder::new()
				// `service_fn` converts our function in a `Service`
				.serve_connection(
					stream, 
					service_fn(move |request| respond(request))
				).await
			{
				error!("Unable to parse the stream as an HTTP request. This can occur when trying to parse a non-HTTP stream: {err}");
			}
		});
	}
}

async fn respond(request: Request<hyper::body::Incoming>) -> Result<Response<HttpBody>, Infallible> {
	// We offload this to another request since the returned result has an
	// Infallible error. We can have more ergonomic error handling by handling it
	// in another function.

	match handle_request(&request).await {
		Ok(response) => {
			debug!("Serving request for: {:?}", request.uri());
			return Ok(response)
		},
		Err(err) => {
			error!("Could not respond to a request for URI `{:?}`. Reason: {err}", request.uri());
			return Ok(err.to_response());
		},
	}
}

async fn handle_request(req: &Request<hyper::body::Incoming>) -> Result<Response<HttpBody>> {
	let config = unsafe { ConfigSettings::get_unchecked() };
	let req_uri = req.uri();
	let req_host = req_uri.host().unwrap_or(&config.request_from);
	let req_path = req_uri.path();
	let req_query = req_uri.query().unwrap_or("");
	let req_method = req.method().to_owned();

	debug!("Received request: {req_method} {req_uri} from User Agent {}", req.headers().get("user-agent").map_or("None", |header| header.to_str().unwrap_or("Corrupted")));
	let server_host = config.request_from.as_str();
	match (req_method, req_host, req_path, req_query) {
		(Method::GET, host, _, _) if host == server_host => {
			let file = read_file_or_index(req_path).await?;
			if !file.may_contain_scripts() {
				Response::builder()
					.status(StatusCode::OK)
					.header(header::CONTENT_TYPE, file.content_type.as_str())
					// TODO: Should include date modified as a header here, to encourage caching.
					.header(header::CACHE_CONTROL, "max-age=86400")  // NOTE(Nate): Just hardcode everything as cachable because I know it is right now.
					.body(file.contents.into())
					.map_err(|http_error| SaraError::FailedToBuildResponse(http_error))
			} else {
				if config.server_side_rendering_enabled {
					let future_doc = parse_text(file.contents)?;
					let document = future_doc.join_all().await;
					Response::builder()
						.status(StatusCode::OK)
						.header(header::CACHE_CONTROL, "max-age=86400") /* NOTE: Sarascript does not current support dynamic dispatch, so I can get away jith manually setting cache ages for now */
						.body(document.contents.into())
						.map_err(|http_error| SaraError::FailedToBuildResponse(http_error))
				} else {
					todo!("Client side parsing is not implemented yet")
				}
			}
		},
		(Method::HEAD, host, _, _) if host == server_host => {
			let file = read_file_or_index(req_path).await?;
			if !file.may_contain_scripts() {
				Response::builder()
					.status(StatusCode::OK)
					.header(header::CONTENT_TYPE, file.content_type.as_str())
					.header(header::CONTENT_LENGTH, file.contents.len())
					// TODO: Should include date modified as a header here, to encourage caching.
					.header(header::CACHE_CONTROL, "max-age=86400")  // NOTE(Nate): Just hardcode everything as cachable because I know it is right now.
					.body("".into())
					.map_err(|http_error| SaraError::FailedToBuildResponse(http_error))
			} else {
				if config.server_side_rendering_enabled {
					let future_doc = parse_text(file.contents)?;
					let document = future_doc.join_all().await;
					Response::builder()
						.status(StatusCode::OK)
						.header(header::CONTENT_LENGTH, document.contents.len())
						.header(header::CACHE_CONTROL, "max-age=86400") /* NOTE: Sarascript does not current support dynamic dispatch, so I can get away jith manually setting cache ages for now */
						.body("".into())
						.map_err(|http_error| SaraError::FailedToBuildResponse(http_error))
				} else {
					todo!("Client side parsing is not implemented yet")
				}
			}
		}
		(other_method, host, _, _) if host == server_host => {
			Err(SaraError::HttpMethodUnsuported(other_method))
		}
		(_, other_host, _, _) => {
			Err(SaraError::HttpHostInvalid(other_host.to_owned()))
		},
	}
}

async fn read_file_or_index(path: &str) -> Result<File> {
	if !path.ends_with("/") {
		// Check if a file named `path`` exists
		match fs::read(path).await {
			Ok(contents) => return Ok(File::new(contents, path)),
			Err(err) => match err.kind() {
				io::ErrorKind::NotFound => (),
				io::ErrorKind::PermissionDenied => return Err(SaraError::FileInvalidPermissions(path.to_owned())),
				_ => if !is_directory(&err) { return Err(SaraError::OtherIOError(err)) },
			}
		};

		// Check if a file named `path`.html exists
		let html_path = format!("{path}.html");
		match fs::read(&html_path).await {
			Ok(contents) => return Ok(File::new(contents, &html_path)),
			Err(err) => match err.kind() {
				io::ErrorKind::NotFound => (),
				io::ErrorKind::PermissionDenied => return Err(SaraError::FileInvalidPermissions(path.to_owned())),
				_ => if !is_directory(&err) { return Err(SaraError::OtherIOError(err)) },
			}
		}

		let index_path = format!("{path}/index.html");
		match fs::metadata(index_path).await { /* Just checking to see if file exists */
			Ok(_) => return Err(SaraError::RequestedDirectory(path.to_owned())),
			Err(_) => return Err(SaraError::FileNotFound(path.to_owned())),
		}

	} else {
		let index_path = format!("{path}index.html");
		match fs::read(&index_path).await {
			Ok(contents) => return Ok(File::new(contents, &index_path)),
			Err(err) => match err.kind() {
				io::ErrorKind::NotFound => return Err(SaraError::FileNotFound(path.to_owned())),
				io::ErrorKind::PermissionDenied => return Err(SaraError::FileInvalidPermissions(path.to_owned())),
				_ => return Err(SaraError::OtherIOError(err)),
			}
		}
	}
}

pub struct HttpBody {
	bytes: Bytes
}

impl Body for HttpBody {
	type Data = Bytes;

	type Error = Infallible;

	fn poll_frame(
		mut self: Pin<&mut Self>,
		_cx: &mut Context<'_>,
	) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
		if !self.bytes.is_empty() {
			let s = std::mem::take(&mut self.bytes);
			Poll::Ready(Some(Ok(Frame::data(s))))
		} else {
			Poll::Ready(None)
		}
	}

	fn is_end_stream(&self) -> bool {
		self.bytes.is_empty()
	}

	fn size_hint(&self) -> SizeHint {
		SizeHint::with_exact(self.bytes.len() as u64)
	}
}

impl<T> From<T> for HttpBody 
where T: Into<Bytes> {
	fn from(value: T) -> Self {
		Self { bytes: value.into() }
	}
}