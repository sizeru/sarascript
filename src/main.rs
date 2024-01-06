use sarascript::server::{launch_server, run_server};
use std::env;

fn main() {
	println!("Starting!");
	let args: Vec<String> = env::args().collect();
	if let Some(flag) = args.get(1) {
		if flag == "-d" {
			run_server();
		}
	}

	launch_server();
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