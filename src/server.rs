use std::{net::SocketAddr, convert::Infallible, sync::Arc};
use anyhow::anyhow;
use daemonize::Daemonize;
use http_body_util::Full;
use log::{info, warn, error};
use tokio::net::TcpListener;
use hyper::{server::conn::http1, Request, Response, service::service_fn, StatusCode, Method, body::Bytes};

use crate::{
	hyper_tokio_adapter::HyperStream,
	parse_file,
	ConfigSettings,
	CONFIG_FILE,
};

const LOCALHOST: [u8; 4] = [127, 0, 0, 1];


pub fn launch_server() {
	let config = ConfigSettings::read(CONFIG_FILE);
	let daemon = create_daemon(&config);

	match daemon.start() {
		Ok(_) => info!("Daemon initialized"),
		Err(e) => panic!("Could not daemonize due to error: {e}"),
	}

	run(config);
}


pub fn create_daemon(config: &ConfigSettings) -> Daemonize<()> {
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
pub async fn run(config: ConfigSettings) {
	let addr = SocketAddr::from((LOCALHOST, config.port)); // Always bind to local loopback.
	let config = Arc::new(config);
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
		let new_config = config.clone();

		// Spawn a tokio task to serve multiple connections concurrently
		tokio::task::spawn(async move {
			// Finally, we bind the incoming connection to our `hello` service
			if let Err(err) = http1::Builder::new()
				// `service_fn` converts our function in a `Service`
				.serve_connection(
					stream, 
					service_fn(move |request| respond(request, new_config.clone()))
				).await
			{
				error!("Error serving connection: {err}");
			}
		});
	}
}

async fn respond(request: Request<hyper::body::Incoming>, config: Arc<ConfigSettings>) -> Result<Response<Full<Bytes>>, Infallible> {
	// We offload this to another request since the returned result has an
	// Infallible error. We can have more ergonomic error handling by handling it
	// in another function.

	match handle_request(&request, config).await {
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

async fn handle_request(req: &Request<hyper::body::Incoming>, config: Arc<ConfigSettings>) -> Result<Response<Full<Bytes>>, anyhow::Error> {
	let uri = req.uri();
	let host = uri.host().unwrap_or(&config.default_host);
	let path = uri.path();
	let query = uri.query().unwrap_or("");

	let _server_host = &config.default_host;
	match (req.method(), host, path, query) {
		(&Method::GET, _server_host, _, "") => {
			if config.server_side_rendering_enabled {
				let document = match parse_file(path).await {
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

fn error_response(err: &anyhow::Error) -> Response<Full<Bytes>> {
	let response = Response::builder()
		.status(StatusCode::INTERNAL_SERVER_ERROR)
		.header("Content-Type", "text/plain")
		.body(Full::new(Bytes::from(err.to_string())))
		.unwrap();
	return response;
}