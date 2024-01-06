use std::{net::SocketAddr, convert::Infallible};
use anyhow::anyhow;
use daemonize::Daemonize;
use http_body_util::Full;
use log::{info, warn, error};
use tokio::{
	net::TcpListener,
	fs,
	io,
};
use hyper::{server::conn::http1, Request, Response, service::service_fn, StatusCode, Method, body::Bytes};

use crate::{
	hyper_tokio_adapter::HyperStream,
	parse_text,
	// parse_text2,
	ConfigSettings,
	CONFIG_PATH,
};

const LOCALHOST: [u8; 4] = [127, 0, 0, 1];

// Run server without creating a daemon (as logged in user). Useful for debugging.
pub fn run_server() {
	let config = ConfigSettings::init(CONFIG_PATH).unwrap();
	std::os::unix::fs::chroot(&config.root).unwrap();
	// TODO: Should init logger to print to stdout
	run();
}

// Create a deamon and launch the server. This shouldbe used in production.
pub fn launch_server() {
	let config = ConfigSettings::init(CONFIG_PATH).unwrap();
	let daemon = create_daemon(&config);

	match daemon.start() {
		Ok(_) => info!("Daemon initialized"),
		Err(e) => panic!("Could not daemonize due to error: {e}"),
	}

	run();
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
async fn run() {
	let config = unsafe { ConfigSettings::get_unchecked() };
	let addr = SocketAddr::from((LOCALHOST, config.port)); // Always bind to local loopback.
	let listener = TcpListener::bind(addr).await.expect("Could not bind TCP Listener");
	// TODO: Would a new struct which implements Tcp Listening be useful?

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
				error!("Error serving connection: {err}");
			}
		});
	}
}

async fn respond(request: Request<hyper::body::Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
	// We offload this to another request since the returned result has an
	// Infallible error. We can have more ergonomic error handling by handling it
	// in another function.

	match handle_request(&request).await {
		Ok(response) => {
			info!("Serving request for: {:?}", request.uri());
			return Ok(response)
		},
		Err(err) => {
			error!("Could not respond to a request for URI `{:?}`. Reason: {err}", request.uri());
			return Ok(error_response(&err))
		},
	}
}

async fn handle_request(req: &Request<hyper::body::Incoming>) -> Result<Response<Full<Bytes>>, anyhow::Error> {
	let config = unsafe { ConfigSettings::get_unchecked() };
	let uri = req.uri();
	let host = uri.host().unwrap_or(&config.default_authority);
	let path = uri.path();
	let query = uri.query().unwrap_or("");

	let _server_host = &config.default_authority;
	match (req.method(), host, path, query) {
		(&Method::GET, _server_host, _, "") => {
			let file_contents = match read_file_or_index(path).await {
				Ok(bytes) => bytes,
				Err(e) => return Err(anyhow!("File could not be accessed or does not exist: {}", e)),
			};
			if config.server_side_rendering_enabled {
				let future_doc = match parse_text(file_contents) {
					Ok(future_doc) => future_doc,
					Err(e) => return Err(anyhow!(format!("Failed to begin parsing document due to: {}", e))),
				};
				let document = match future_doc.join_all().await {
					Ok(doc) => doc,
					Err(e) => return Err(anyhow!(format!("Could not parse document due to: {}", e)))
				};
				return Ok(Response::new(Full::new(Bytes::from(document.contents))));
			} else {
				return Err(anyhow!("Client side rendering is not implemented yet"));
			}
		},
		_ => {
			return Err(anyhow!(format!("I don't know how to respond to the requested uri")));
		},
	}
}

async fn read_file_or_index(path: &str) -> Result<Vec<u8>, io::Error> {
	// Check if a file named `path`` exists
	if let Ok(bytes) = fs::read(path).await { return Ok(bytes) } 
	// Check if a file named `path`.html exists
	if !path.ends_with("/") {
		if let Ok(bytes) = fs::read(format!("{path}.html")).await { return Ok(bytes) } 
	}
	// Check if either `path`/index.html or `path`index.html exists
	let index_file = if path.ends_with("/") { format!("{path}index.html") } else { format!("{path}/index.html") };
	return fs::read(index_file).await;
}

fn error_response(err: &anyhow::Error) -> Response<Full<Bytes>> {
	let response = Response::builder()
		.status(StatusCode::INTERNAL_SERVER_ERROR)
		.header("Content-Type", "text/plain")
		.body(Full::new(Bytes::from(err.to_string())))
		.unwrap();
	return response;
}
