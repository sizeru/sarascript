use sarascript::server::{launch_server, run_server};
use std::{env, process::ExitCode};

fn main() -> Result<ExitCode, sarascript::error::SaraError> {
	println!("Starting sarascript!");
	let args: Vec<String> = env::args().collect();
	let debug_flag = args.get(1).is_some_and(|flag| flag == "-d");

	let exit_status = if debug_flag {
		simplelog::WriteLogger::init(
			simplelog::LevelFilter::Debug,
			simplelog::Config::default(),
			std::io::stdout()
		).expect("Could not initialize logger");
		run_server()
	} else {
		launch_server()
	};

	return exit_status;
}

// #[tokio::main]
// async fn main() {
// 	let response = parse_file("example.html").await.unwrap();
// 	let response_string = String::from_utf8(response.contents).unwrap();
// 	println!("Response: \n{response_string}");


// 	// let res = task::block_on(get(vec![domain, "/ip".to_owned()])).unwrap();
// 	// let res_string = String::from_utf8(res).unwrap();
// 	// println!("Response1: \n{res_string}");
// }