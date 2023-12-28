use htmlprep::get;
use async_std::task;

fn main() {
	let mut domain = String::new();
	domain.push_str("httpbin.org");
	let response = task::block_on(get(&domain, "/get")).unwrap();
	// let response_string = String::from_utf8(response).unwrap();
	// println!("Response: \n{response_string}");
}