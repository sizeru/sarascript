use htmlprep::parse_file;
use async_std::task;

fn main() {
	let domain = String::from("httpbin.org");
	let response = task::block_on(parse_file("simple.html")).unwrap();
	let response_string = String::from_utf8(response).unwrap();
	println!("Response: \n{response_string}");

	// let res = task::block_on(get(vec![domain, "/ip".to_owned()])).unwrap();
	// let res_string = String::from_utf8(res).unwrap();
	// println!("Response1: \n{res_string}");
}