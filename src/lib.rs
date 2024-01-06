use std::{
	str,
	sync::Arc,
	ops::Range,
};
use log::info;
use tokio::{
	sync::{Mutex, OnceCell},
	task::{self, JoinHandle},
	io::AsyncWriteExt,
};
use http::{Request, Uri};
use hyper::body::Bytes;
use pest_derive::Parser;
use pest::{Parser, iterators::Pair};
use http_body_util::{Empty, BodyExt};
use config::{Config, FileFormat};

pub mod hyper_tokio_adapter;
use hyper_tokio_adapter::HyperStream;
#[cfg(feature = "server")]
pub mod server;
mod error;
use error::SaraError;

type Result<T, E = SaraError> = std::result::Result<T, E>;

const USER_AGENT: &str = concat!("sara/", env!("CARGO_PKG_VERSION"));
const LOG_FILE: &str = "/var/log/sarascriptd/sarascriptd.log";
const PID_FILE: &str = "/var/run/sarascriptd.pid";
const CONFIG_PATH: &str = "/etc/sarascriptd.conf";
static CONFIG: OnceCell<ConfigSettings> = OnceCell::const_new();

#[derive(Parser)]
#[grammar = "sarascript.pest"]
pub struct SaraParser;

#[derive(Debug)]
struct ConfigSettings {
	user: String,
	root: String,
	default_authority: String,
	server_side_rendering_enabled: bool,
	port: u16,
	client_side_rendering_enabled: bool,
	log_filename: String,
	pid_filename: String,
	certificate_authorities_filename: String,
}

impl ConfigSettings {
	// Read and parse a config from the filesystem. This should probably only ever be called once.
	pub fn read(config_filename: &str) -> Result<ConfigSettings> {
		let config = Config::builder()
			.add_source(config::File::new(config_filename, FileFormat::Toml))
			.build()?;
		return ConfigSettings::parse(config);
	}

	pub async fn read_async(config_filename: &str) -> Result<ConfigSettings> {
		let config_filename_clone = config_filename.to_owned();
		let config = task::spawn_blocking(move || {
			Config::builder()
				.add_source(config::File::new(&config_filename_clone, FileFormat::Toml))
				.build()
		}).await??;
		return ConfigSettings::parse(config);
	}

	fn parse(config: Config) -> Result<ConfigSettings> {
		let root = config.get_string("root")?;
		let user = config.get_string("user")?;
		let default_authority = config.get_string("default_authority")?;
		let client_side_rendering_enabled = config.get_bool("client_side_rendering").unwrap_or(false);
		let server_side_rendering_enabled = config.get_bool("server_side_rendering").unwrap_or(false);
		let port = config.get_int("port")? as u16; 
		let log_filename = config.get_string("log_filename").unwrap_or(LOG_FILE.to_string());
		let pid_filename = config.get_string("pid_filename").unwrap_or(PID_FILE.to_string());
		let cafile = config.get_string("certificate_authorities")?;

		let config = ConfigSettings {
			user,
			root,
			default_authority,
			server_side_rendering_enabled,
			port,
			client_side_rendering_enabled,
			log_filename,
			pid_filename,
			certificate_authorities_filename: cafile,
		};

		return Ok(config);
	}

	pub fn init(config_filename: &str) -> Result<&'static Self> {
		let config = Self::read(config_filename)?;
		CONFIG.set(config)?;
		let config = unsafe { CONFIG.get().unwrap_unchecked() };
		return Ok(config);
	}

	pub async fn get() -> Result<&'static Self> {
		let config = CONFIG.get_or_try_init(|| async {
			Self::read_async(CONFIG_PATH).await
		}).await;
		return config;
	}

	pub unsafe fn get_unchecked() -> &'static Self {
		unsafe { CONFIG.get().unwrap_unchecked() }
	}
}


pub struct Document {
	pub contents: Vec<u8>
}

impl Document {}

struct FutureDocument {
	join_handles: Vec<JoinHandle<Result<()>>>,
	wip_document: Arc<Mutex<WipDocument>>,
	_current_handle_index: usize, // Used when polling
}

impl FutureDocument {
	fn new(wip_doc: Arc<Mutex<WipDocument>>, join_handles: Vec<JoinHandle<Result<()>>>) -> Self {
		Self {
			join_handles,
			wip_document: wip_doc,
			_current_handle_index: 0,
		}
	}

	async fn join_all(mut self) -> Result<Document> {
		for handle in &mut self.join_handles {
			_ = handle.await?;
		}

		let mutex = Arc::into_inner(self.wip_document).unwrap(); // Known to be safe
		let doc = mutex.into_inner(); // Known to be safe
		return Ok( Document { contents: doc.contents });
	}
}

pub async fn parse_file(filename: &str) -> Result<Document> {
	let bytes = tokio::fs::read(filename).await?;
	let document = parse_text(bytes)?.join_all().await?;
	return Ok(document);
}

// Take ownership of the vec to be more effecient.
fn parse_text(text: Vec<u8>) -> Result<FutureDocument> {
	let original_text_string = std::str::from_utf8(&text)?;
	let parsed_file = SaraParser::parse(Rule::file, original_text_string)?;

	let wip_doc = Arc::new(Mutex::new(WipDocument::new(text.clone())));
	let mut join_handles = Vec::new();

	for script in parsed_file {
		let operations = parse_script(script);
		for operation in operations {
			join_handles.push(task::spawn( execute_operation(operation, wip_doc.clone())));
		}
	}

	let future_doc = FutureDocument::new(wip_doc, join_handles);
	return Ok(future_doc);
}

async fn execute_operation(operation: Operation, wip_document: Arc<Mutex<WipDocument>>) -> Result<()> {
	match operation.opcode {
		Opcode::GET => {
			let resource = match get(operation.args).await {
				Ok(resource) => resource,
				Err(e) => format!("Could not load resource: {:?}", e.to_string()).as_bytes().to_vec(),
			};
			let mut doc = wip_document.lock().await;
			doc.insert(resource, operation.span);
		},
		Opcode::TAG => {
			let mut doc = wip_document.lock().await;
			doc.remove(operation.span);
		},
		Opcode::ERR => unreachable!(),
	};
	return Ok(());
}

#[derive(Eq)]
pub struct DocEdit {
	original_index: usize,
	edit_length: i64,
}

impl Ord for DocEdit {
	fn cmp(&self, other: &Self) -> std::cmp::Ordering {
		return self.original_index.cmp(&other.original_index);
	}
}

impl PartialOrd for DocEdit {
	fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
		match self.original_index.partial_cmp(&other.original_index) {
			Some(core::cmp::Ordering::Equal) => {}
			ord => return ord,
		}
		self.edit_length.partial_cmp(&other.edit_length)
	}
}

impl PartialEq for DocEdit {
	fn eq(&self, other: &Self) -> bool {
		return self.original_index == other.original_index;
	}
}

pub struct WipDocument {
	contents: Vec<u8>,
	edits: Vec<DocEdit>, // This vector is sorted
}

impl WipDocument {
	pub fn new(doc: Vec<u8>) -> Self {
		WipDocument {
			contents: doc,
			edits: Vec::new(),
		}	
	}
}

impl WipDocument {
	fn add_edit(&mut self, original_index: usize, edit_size: i64) {
		let edit = DocEdit {original_index, edit_length: edit_size};
		if let Err(insert_index) = self.edits.binary_search(&edit) {
			self.edits.insert(insert_index, edit);
			return;
		}
		unreachable!(); // If the Ok variant is reached, there is already an edit beginning from this exact index. This is impossible.
	}

	fn get_wip_index(&self, original_index: usize) -> usize {
		// Binary search will return the first DocEdit on or after the index in the
		// original document. By summing up all edits before this, we can calculate
		// just how much the document has shifted.
		let doc_index = match self.edits.binary_search(&DocEdit{original_index, edit_length:0}) {
			Ok(index) => index,
			Err(index) => index,
		};
		let edits = &self.edits[0..doc_index];
		let mut sum = 0;
		for edit in edits {
			sum += edit.edit_length
		}
		return original_index.wrapping_add(sum as usize);
	}

	// Insert a resource into this document
	// TODO: The as [integers] in this function have the potential for undefined behavior
	pub fn insert(&mut self, resource: Vec<u8>, range: Range<usize>) { 
		let wip_index = self.get_wip_index(range.start);
		self.add_edit(range.start, resource.len() as i64 - range.len() as i64);
		self.contents.splice(wip_index..wip_index+range.len(), resource);
	}

	pub fn remove(&mut self, range: Range<usize>) {
		let wip_index = self.get_wip_index(range.start);
		self.add_edit(range.start, range.start.wrapping_sub(range.end) as i64);
		self.contents.drain(wip_index..wip_index+range.len());
	}
}

struct Script {
	operations: Vec<Operation>,
	is_type_correct: bool,
}

impl Script {
	// Returns whether the script is valid sarascript
	fn is_okay(&self) -> bool {
		if self.is_type_correct == false {
			return false;
		}

		for operation in &self.operations {
			match operation.opcode {
				Opcode::TAG => continue, // Parsing will only allow two tag operations
				Opcode::ERR => return false,
				Opcode::GET => {
					if operation.args.len() != 1 { // Could do more advanced checking here
						return false;
					}
				}
			}
		}

		return true;
	}

	// Returns a new uninitialized script. This should be initialized
	fn uninit() -> Self {
		Self {
			operations: Vec::new(),
			is_type_correct: false,
		}
	}
}

fn parse_script(script: Pair<'_, Rule>) -> Vec<Operation> {
	let mut parsed_script = Script::uninit();
	let operations = &mut parsed_script.operations;
	
	for rule in script.into_inner() {
		match rule.as_rule() {
			Rule::function => {
				let span = rule.as_span().start()..rule.as_span().end();
				let mut function_data = rule.into_inner();
				let name = function_data.next().unwrap().as_str();
				let opcode = match name {
					"get" => Opcode::GET,
					_ => Opcode::ERR,
				};
				let mut args = Vec::new();
				for argument in function_data {
					let data = argument.into_inner().next().unwrap();
					let value = match data.as_rule() {
						Rule::symbol => Arg::Symbol(data.as_str().to_owned()),
						Rule::string => Arg::String(data.into_inner().next().unwrap().as_str().to_owned()),
						Rule::number => Arg::Num(data.as_str().parse().unwrap()),
						_ => unreachable!()
					};
					args.push(value);
				}
				operations.push(Operation { opcode, args, span });
			},
			Rule::script_opening_tag => {
				let start = rule.as_span().start();
				let end = rule.as_span().end();
				let op = Operation {
					opcode: Opcode::TAG,
					args: Vec::new(),
					span: start..end,
				};
				operations.push(op);

				for attribute in rule.into_inner() {
					let mut attribute_info = attribute.into_inner();
					let name = attribute_info.next().unwrap().as_str(); // Guaranteed
					let text = attribute_info.next().unwrap().into_inner().next().unwrap().as_str(); // Guaranteed
					if name == "type" && text == "sarascript" {
						parsed_script.is_type_correct = true;
					}
				}
			},
			Rule::script_closing_tag => {
				let start = rule.as_span().start();
				let end = rule.as_span().end();
				let op = Operation {
					opcode: Opcode::TAG,
					args: Vec::new(),
					span: start..end,
				};
				operations.push(op);
			}
			_ => unreachable!(),
		}
	}
	
	// We want to make sure the entire script is okay before beginning to dispatch
	assert!(parsed_script.is_okay());
	return parsed_script.operations;
}

#[derive(Debug)]
enum Opcode {
	GET,
	ERR,
	TAG, // Used internally as an instruction which should delete the internet 'tag' marks from the script
}

#[derive(Debug)]
struct Operation {
	opcode: Opcode,
	span: Range<usize>, // What text is this operation meant to replace
	args: Vec<Arg>,
}

#[derive(Debug)]
enum Arg {
	String(String),
	Num(i64),
	Symbol(String),
}

impl Arg {
	// TODO: This could be more effecient, but would be a micro-optimization
	unsafe fn into_inner_string(&self) -> String {
		match self {
			Arg::String(string) => string.clone(),
			_ => unreachable!()
		}
	}

	unsafe fn into_inner_num(&self) -> i64 {
		match self {
			Arg::Num(num) => num.clone(),
			_ => unreachable!()
		}
	}

	unsafe fn into_inner_symbol(&self) -> String {
		match self {
			Arg::Symbol(symbol) => symbol.clone(),
			_ => unreachable!()
		}
	}
}

async fn get(args: Vec<Arg>) -> Result<Vec<u8>> {
	// TODO: Connection should be reused when possible
	let config = ConfigSettings::get().await?;
	let uri_string = unsafe { args.get(0).unwrap_unchecked().into_inner_string() }; // Safe due to parsing guarantees
	let uri: Uri = uri_string.parse()?;
	info!("sarascript::get() -> {}", uri.to_string());
	let config_authority: Uri = config.default_authority.parse()?;
	let mut is_using_config_host = false;
	let host = uri.host().unwrap_or_else(|| {
		is_using_config_host = true;
		config_authority.host().unwrap() // TODO: Need to check config before getting to assert that it is okay
	});

	let port = match uri.port_u16() {
		Some(port) => port,
		None => {
			if is_using_config_host && config_authority.port_u16().is_some() {
				unsafe { config_authority.port_u16().unwrap_unchecked() }
			} else {
				443
			}
		},
	};
	let path_and_query = match uri.path_and_query() {
		Some(path_and_query) => path_and_query.as_str(),
		None => "/"
	};

	let use_tls = if port == 443 { true } else { false };
	let stream = HyperStream::connect(host, port, use_tls).await?;

	info!("Processing request: GET {host}:{port}{path_and_query}");

	// Perform a TCP handshake
	let (mut sender, conn) = hyper::client::conn::http1::handshake(stream).await.unwrap();

	// Spawn task to poll the connection, driving the Http state.
	// This `conn` will only return when the connection is closed
	// Note: I feel like not awaiting this is possible race condition.
	task::spawn(async move {
		if let Err(err) = conn.await {
			println!("Connection failed: {:?}", err);
		}
	});

	let request = Request::builder()
		.uri(path_and_query)
		.header(hyper::header::USER_AGENT, USER_AGENT)
		.header(hyper::header::HOST, host)
		// .header(hyper::header::CONNECTION, "close")
		.body(Empty::<Bytes>::new())?;

	// Await the response...
	let mut response = sender.send_request(request).await?;

	let mut text = Vec::new();
	while let Some(next) = response.frame().await {
		let frame = next?;
		if let Some(chunk) = frame.data_ref() {
			text.reserve(chunk.len());
			text.write(chunk).await?;
		}
	}

	return Ok(text);
}

// async fn connect(host: &str, port: Option<u16>) -> 
// Result<http1::SendRequest<Empty<Bytes>>> {
// 	// Use an adapter to implement `hyper::rt` IO traits.
// 	let port = port.unwrap_or(443);
// 	let use_tls = if port == 443 { true } else { false };
// 	let stream = task::block_in_place(move || {HyperStream::connect(host, port, use_tls)}).await?;

// 	// Perform a TCP handshake
// 	let (sender, conn) = hyper::client::conn::http1::handshake::<HyperStream, Empty<Bytes>>(stream).await.unwrap();

// 	// Spawn a task to poll the connection, driving the HTTP state. Should this be async?
// 	// Note: I feel like not awaiting this is a race condition.
// 	task::spawn(async move {
// 		if let Err(err) = conn.await {
// 			println!("Connection failed: {:?}", err);
// 		}
// 	});

// 	return Ok(sender);
// }
	
// pub async fn get_single<'a, B: Body + 'static>(sender: Arc<Mutex<http1::SendRequest<B>>>, body: B, domain: String, path: String) -> Result<Response<Incoming>>{
// 	let mut sender  = sender.lock().await;
// 	let request = Request::builder()
// 		.uri(path)
// 		.header(hyper::header::USER_AGENT, USER_AGENT)
// 		.header(hyper::header::HOST, &domain)
// 		// .header(hyper::header::CONNECTION, "close")
// 		.body(body)?;

// 	// Await the response...
// 	let response = sender.send_request(request).await?;
// 	return Ok(response);
// }
