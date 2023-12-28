use htmlprep::get;
use async_std::task;

fn main() {
	let response = task::block_on(get("httpbin.org", "/headers")).unwrap();
	let response_string = String::from_utf8(response).unwrap();
	println!("Response: \n{response_string}");
}