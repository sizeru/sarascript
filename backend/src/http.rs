use std::io::prelude::*;
use std::net::TcpStream;
use std::collections::{HashMap};
use crate::Body::{Single}; //NOTE: See [2]

// HTTP Response Codes
pub const BAD_REQUEST: Response = Response{code:"400 Bad Request", headers:None, message:None} ;
pub const OK: Response = Response{code:"200 OK", headers:None, message:None};
pub const CREATED: Response = Response{code:"201 Created", headers:None, message:None};
pub const ACCEPTED: Response = Response{code:"202 Accepted", headers:None, message:None};
pub const INTERNAL_SERVER_ERROR: Response = Response{code:"500 Internal Server Error", headers:None, message:None};
pub const NOT_IMPLEMENTED: Response = Response{code:"501 Not Implemented", headers:None, message:None};
pub const CONTENT_TOO_LARGE: Response = Response{code:"413 Content Too Large", headers:None, message:None};
pub const LENGTH_REQUIRED: Response = Response{code:"411 Length Required", headers:None, message:None};
pub const SERVICE_UNAVAILABLE: Response = Response{code: "503 Service Unavailable", headers:None, message:None};
pub const UNAUTHORIZED: Response = Response{code: "401 Unauthorized", headers:None, message:None};
pub const FOUND: Response = Response{code: "302 Found", headers:None, message:None};

// Frequently used content types
pub const HTML: &str = "text/html";
pub const TEXT: &str = "text/plain; charset=utf-8";
pub const CSS: &str = "text/css";
pub const PDF: &str = "application/pdf";
// RUST WEBSERVER CONSTANTS
const REQUEST_BUFFER_SIZE: usize = 4096;

// println which only appears in debug mode
#[cfg(debug_assertions)]
macro_rules! debug_println {
    ($($input:expr),*) => {
        println!($($input),+);
    };
}
#[cfg(not(debug_assertions))]
macro_rules! debug_println {
    ($($input:expr),*) => {()}
}

// Information Needed to Create an HTTP Response
#[derive(Debug)]
pub struct Response {
    code: &'static str,
    headers: Option<HashMap<&'static str, String>>,
    message: Option<Vec<u8>>,
}

// This contains information about an HttpLine
struct LineBuffer {
    index: usize,
    size: usize,
    buffer: [u8; HttpRequest::MAX_LINE_LENGTH],
}

pub struct RequestBuffer {
    pub index: usize,
    pub size: usize,
    pub buffer: [u8; REQUEST_BUFFER_SIZE],
}

impl RequestBuffer {
    pub fn new() -> RequestBuffer {
        RequestBuffer { size: 0, index: 0, buffer: [0; REQUEST_BUFFER_SIZE] }
    }

    // TODO: This function needs review and documentation
    pub fn fill(&mut self, stream: &mut TcpStream) -> Result<usize, Response> {
        self.index = 0;
        let num_bytes_read = stream.read(&mut self.buffer);
        if let Err(error) = num_bytes_read {
            return Err(INTERNAL_SERVER_ERROR.clone_with_message(error.to_string()));
        }
        let num_bytes_read = num_bytes_read.unwrap();
        self.size = num_bytes_read;
        if num_bytes_read == 0 {
            // TODO: This means the TcpStream is closed. Figure out what to do here.
            return Ok(num_bytes_read);
        }
        return Ok(num_bytes_read);
    }
}

// Functions for manipulating repsonses
impl Response {

    // Generates an HTTP Response Message
    fn format(&self) -> Vec<u8> {
        let mut response: Vec<u8> = Vec::new();
        response.append(&mut format!("HTTP/1.1 {}\r\n", self.code).as_bytes().to_vec());
        if let Some (headers) = &self.headers {
            for (key, value) in headers {
                response.append(&mut format!("{}: {}\r\n", key, value).as_bytes().to_vec());       
            }
            if !headers.contains_key("content-type") {
                response.append(&mut "content-Type: text/plain; charset=utf-8\r\n".as_bytes().to_vec());
            }
        } else {
            response.append(&mut "content-type: text/plain; charset=utf-8\r\n".as_bytes().to_vec());
        }
        if let Some(message) = self.message.as_ref(){
            let content_length = message.len() + 2;
            response.append(&mut format!("Content-Length: {}\r\n\r\n",content_length).into_bytes().to_vec());
            response.append(&mut message.clone());
            response.append(&mut "\r\n".as_bytes().to_vec());
        }
        
        return response;
    }

    // Adds a header to the response
    pub fn add_header(&mut self, key: &'static str, value: String) {
        if let Some(headers) = &mut self.headers {
            headers.insert(key, value);
        } else {
            let mut headers = HashMap::with_capacity(1);
            headers.insert(key, value);
            self.headers = Some(headers);
        }
    }

    // Adds a message to the response
    pub fn add_message(&mut self, message: Vec<u8>) {
        self.message = Some(message);
    }

    // Sends an HTTP Response back to the client
    pub fn send(&self, client_stream: &mut TcpStream) {
        // This will make many repeated calls to write to the stream. It is easier to write the content this way

        if let Err(error) = client_stream.write(&self.format().as_slice()) {
            println!("ERROR WHEN WRITING RESPONSE: {}", error.to_string());
        }
        if let Err(error) = client_stream.flush() {
            println!("ERROR WHEN SENDING RESPONSE: {}", error.to_string());
        }   
        
    }

    // Clone this response with a different message
    pub fn clone_with_message(&self, message: String) -> Response {
        let headers = self.headers.clone();
        return Response{code: self.code, headers, message: Some(message.as_bytes().to_vec())};
    }
}

// When FormData is recieved in an HTTP request, each FormData contains
// headers and a body. There may be several of these FormDatas, each of which
// may contain another nested formdata entry. This is the recursive struct which defines
// them.
#[derive(Debug)]
struct FormData {
    headers: HashMap<String, String>,
    body: Body
}

// The body of an HTTP request will be stored as either a raw of bytes, or a
// struct called FormData which is used as the value for a MultiPart enum. More
// info about FormData can be found in its definition.
#[derive(Debug)]
pub enum Body {
    Single(Vec<u8>), // Raw byte stream. Used when receiving raw data. TODO: This may be able to take a fixed size array instead of a Vector.
    MultiPart(Vec<FormData>), // Struct with headers. Used when receiving multipart data.
    None
}

impl Body {
    pub fn single(&self) -> Result<&Vec<u8>, &str> {
        if let Single(body) = self {
            return Ok(body);
        } else {
            return Err("Could not convert Body struct to byte array");
        }
    }
}

// All relevant information in an HttpRequest
#[derive(Debug)]
pub struct HttpRequest { 
    pub method: String,
    pub location: String,
    pub query: HashMap<String, String>,
    pub version: String,
    pub headers: HashMap<String, String>,
    pub body: Body // Body is typically stored as a raw byte array
}

impl HttpRequest {
    pub const MAX_LINE_LENGTH: usize = 1024; // The maximum bytes allowed between /r/n sequences (excluding the Body)
    pub const MAX_HEADERS: usize = 32;

    pub fn parse(stream: &mut TcpStream, request_buffer: &mut RequestBuffer) -> Result<HttpRequest, Response> {
        let mut total_bytes_read: usize = 0;
        let mut line_buffer = LineBuffer{ index: 0, size: 0, buffer: [0; HttpRequest::MAX_LINE_LENGTH] };
        request_buffer.size = 0;

        // Control Data Line
        total_bytes_read += get_http_line(stream, request_buffer, &mut line_buffer)?;
        let mut request = parse_control_data_line(&line_buffer)?;
        
        // Headers
        while let bytes_read = get_http_line(stream, request_buffer, &mut line_buffer)? {
            if bytes_read == 0 {break;} // 0 length header means body
            total_bytes_read += bytes_read;

            // Parse header and append header
            let (key, value) = parse_header_line(&line_buffer)?;
            let old_header = request.headers.get_mut(&key);
            
            if let Some(header) = old_header {
                header.push_str(", ");
                header.push_str(&value);
            } else {
                request.headers.insert(key, value);
            }

            // Check if headers over limit
            if request.headers.len() > HttpRequest::MAX_HEADERS {
                return Err(CONTENT_TOO_LARGE.clone_with_message(
                    format!("This server will not accept a request with more than {} headers.", HttpRequest::MAX_HEADERS)
                ));
            }
        }


        // Check for body (Require Content-Length header)
        debug_println!("<== PARSE BODY ==>");
        debug_println!("Request up to this point: {:#?}", request);
        // Convert content_length into usize
        let content_length = request.headers.get("Content-Length");
        if content_length.is_none() {
            #[cfg(feature = "echo-test")]
            if request.location.starts_with("/api/echo") {
                return Err(OK.clone_with_message(format!("{:#?}", request)));
            }
            return Ok(request);
        }
        let content_length = (*content_length.unwrap()).parse::<usize>();
        if let Err(error) = content_length {
            return Err(BAD_REQUEST.clone_with_message("Could not parse the 'Content-Length' header as an integer".to_string()));
        }
        let content_length = content_length.unwrap();

        // Create body and read until content_length is reached or error occurs
        request.body = Single(Vec::with_capacity(content_length));
        if let Single(body) = &mut request.body { // Not a fan of this syntax here, but I get a mutability error any other way
            loop {
                // Append bytes from buffer to body
                let buffer_bytes = &mut request_buffer.buffer[request_buffer.index..request_buffer.size].to_vec();
                body.append(buffer_bytes);

                // Fetch for more bytes if not enough
                if body.len() >= content_length {
                    break;
                }
                if request_buffer.fill(stream)? == 0 {
                    return Err(BAD_REQUEST.clone_with_message(
                        format!("Received 'Content-Length: {}', but only read {} bytes", content_length, body.len())
                    ));
                }
            }
        }
        // debug_println!("Body parsed. Request up to this point: {:?}", request);

        #[cfg(feature = "echo-test")]
        if request.location.starts_with("/api/echo") {
            return Err(OK.clone_with_message(format!("{:#?}", request)));
        }

        return Ok(request);
    }
    
    fn new(method: String, location: String, query: HashMap<String, String>, version: String) -> HttpRequest {
        HttpRequest {
            method: method,
            location: location,
            query: query,
            version: version,
            headers: HashMap::with_capacity(1), // Any HTTP request in HTTP/1.1 and above must have the Host header
            body: Body::None, // AKA Content
        }
    }
}

// NOTE: Although http requests are meant to be delimited by CRLF, I think
// this is stupid. Having a two byte delimiter makes everything more
// complicated than it needs to be. RFC7230 whic appends section 3.5 of the
// HTTP protocol states that "we recommend that applications, when parching
// such headers, recognize a single LF as a line terminator and ignore the
// leading CR." This is exactly what I will do. This ambiguity may lead to
// some potential vulnerabilities. A diagram showing the logic  
fn get_http_line(stream: &mut TcpStream, request_buffer: &mut RequestBuffer, line: &mut LineBuffer) -> Result<usize, Response> {
    let mut end: usize;
    line.size = 0;
    loop {
        // From the buffer, append all to line until either LF or End of buffer is reached
        if request_buffer.index >= request_buffer.size {
            let size = request_buffer.fill(stream)?;
            debug_println!("==> LOADED BUFFER OF SIZE {}: <==\n{:?}", size, String::from_utf8_lossy(&request_buffer.buffer[0..request_buffer.size]));
        }
        let lf_index = (&request_buffer.buffer[request_buffer.index..request_buffer.size]).iter().position(|&x| x == b'\n');
        end = if let Some(index) = lf_index {index + request_buffer.index} else {request_buffer.size};
        
        let length_read = end - request_buffer.index;
        let new_line_size = line.size + length_read;
        if new_line_size >= line.buffer.len() {
            return Err(CONTENT_TOO_LARGE.clone_with_message(
                format!("Server will not accept a message with a line more than {} bytes long", line.buffer.len())
            ));
        }

        // Copy contents to correct place in line buffer
        line.buffer[line.size..new_line_size].copy_from_slice(&request_buffer.buffer[request_buffer.index..end]);
        request_buffer.index = end + 1;
        line.size = new_line_size;
        
        // Check for lf and leave, or repeat the process if not
        if lf_index.is_none() {
            continue;
        }
        if line.size == 0 {
            return Err(BAD_REQUEST.clone_with_message(
                "A line contains only a single line feed. This is not allowed on this server".to_string()
            ));            
            // TODO: There is no reason this should not be continue; instead of a server error, but just for initial testing I'll leave it like this.
        }
        let last_byte = line.buffer[line.size - 1];
        if last_byte != b'\r' {
            return Err(BAD_REQUEST.clone_with_message(
                "There was a dangling LF (line feed). This is not permitted for this server".to_string()
            ));            
            // TODO: There is no reason this should not be continue; instead of a server error, but just for initial testing I'll leave it like this.
        }

        // SUCCESS. ALL CHECKS PASSED
        line.size -= 1; // Don't want the \r in the stream
        debug_println!("==> LINE READ FROM BUFFER: {:?} <==", String::from_utf8_lossy(&line.buffer[..line.size]));
        return Ok(line.size);
    }
}

fn parse_control_data_line (line_buffer: &LineBuffer) -> Result<HttpRequest, Response> {
    let line = &line_buffer.buffer[..line_buffer.size];
    let line = String::from_utf8(line.to_vec());
    if let Err(FromUtf8Error) = line {
        return Err(BAD_REQUEST.clone_with_message("Control Line contained not UTF-8 characters".to_string()));
    }
    let line = line.unwrap();
    let values: Vec<&str> = line.split_ascii_whitespace().collect();
    if values.len() != 3 {
        return Err(BAD_REQUEST.clone_with_message("Control line was improperly formatted".to_string()));
    }
    
    let method = values[0].to_string();
    let (location, query) = parse_location(&values[1].to_string())?;
    if values[2].len() < 6 {
        return Err(BAD_REQUEST.clone_with_message("HTTP version was improperly formatted".to_string()));
    }
    let version = (&values[2][5..]).to_string();

    return Ok(HttpRequest::new(method, location, query, version));
}

pub fn parse_location(location_line: &String) -> Result<(String, HashMap<String, String>), Response> {
    let relative_location;
    if let Some(index) = location_line.find("://") {
        let index = index + 3;
        relative_location =  &location_line[index..];
    } else {
        relative_location = location_line;
    }
    let values: Vec<&str> = relative_location.split("?").collect();
    let location = values[0].to_string();
    let location = urlencoding::decode(&location);
    if let Err(error) = location { return Err(BAD_REQUEST.clone_with_message(format!("The location in the url could not be decoded: {}", error.to_string()))); }
    let location = location.unwrap().into_owned();

    let mut queries: HashMap<String, String> = HashMap::new();
    match values.len() {
        1 => {
            return Ok((location, queries));
        }
        2 => {/* break */}
        _ => {
            return Err(BAD_REQUEST.clone_with_message("Cannot have more than two '?' characters in a URL".to_string()));
        }
    }

    let pairs: Vec<&str> = values[1].split("&").collect();
    for pair in pairs {
        let value: Vec<&str> = pair.split("=").collect();
        if value.len() == 1 {
            continue;
        }
        queries.insert(value[0].to_string(), value[1].to_string());
    }
    return Ok((location, queries));
}

fn parse_header_line<'a> (line_buffer: &LineBuffer) -> Result<(String, String), Response> {
    let line = &line_buffer.buffer[..line_buffer.size];
    let line = String::from_utf8(line.to_vec());
    if let Err(FromUtf8Error) = line {
        return Err(BAD_REQUEST.clone_with_message("A header line contained not UTF-8 characters".to_string()));
    }
    let line = line.unwrap();
    let values: Vec<&str> = line.split(": ").collect();
    if values.len() != 2 {
        return Err(BAD_REQUEST.clone_with_message("A header line was improperly formatted".to_string()));
    }
    return Ok((values[0].to_string(), values[1].to_string()));
}