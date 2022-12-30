use std::fs::{File, self};
use std::hash::Hash;
use std::io::{prelude::*};
use std::net::{TcpListener, TcpStream};
use std::collections::{HashMap};
use std::path::Path;
use std::string::FromUtf8Error;
use chrono::{Utc, TimeZone, DateTime, Date, Duration};
use chrono_tz::Asia::Vladivostok;
use chrono_tz::Etc::{GMTPlus4};
use compress::zlib;
use num_digitize::FromDigits;
use postgres::{Client, NoTls};
use urlencoding::encode;
use crate::Body::{Single}; //NOTE: See [2]

// RUST WEBSERVER CONSTANTS
const REQUEST_BUFFER_SIZE: usize = 4096;
const LOCAL: &str = "127.0.0.1:7878";
const IPV4: &str = "45.77.158.123:7878";
const IPV6: &str = "[2001:19f0:5:5996&:5400:4ff:fe02:3d3e]:7878";
const PDFS_FILEPATH: &str = "/home/nate/code/rmc/site/belgrade/documents/";
const POSTGRES_ADDRESS: &str = "postgresql://nate:testpasswd@localhost/rmc";
const CR: &[u8] = &[13 as u8];
const LF: &[u8] = &[10 as u8];
// const ROOT_DIR: &str = "https://redimixdominica.com/belgrade/documents/";


// HTTP Response Codes
const BAD_REQUEST: Response = Response{code:"400 Bad Request", message:None} ;
const OK: Response = Response{code:"200 OK", message:None};
const CREATED: Response = Response{code:"201 Created", message:None};
const ACCEPTED: Response = Response{code:"202 Accepted", message:None};
const INTERNAL_SERVER_ERROR: Response = Response{code:"500 Internal Server Error", message:None};
const NOT_IMPLEMENTED: Response = Response{code:"501 Not Implemented", message:None};
const CONTENT_TOO_LARGE: Response = Response{code:"413 Content Too Large", message:None};
const LENGTH_REQUIRED: Response = Response{code:"411 Length Required", message:None};

// [1] The current request buffer size is 4KB, the pagesize on the computer I'm
// running the server on (and most Linux servers as of 2022 Dec). In theory,
// memory aligned data speeds up data access by keeping the cache hot, and makes
// the maximal use of memory, but I can't help but feel that I'm missing
// something. More research and testing needs to be done to find the optimal
// request buffer size.

// [2] As of 2022 Dec, receiving multipart/formdata is not supported, and is
// very low priority. All data must be sent as a binary stream in the body of a
// request.

// Formats an HTTP response to an array of bytes.
macro_rules! response {
    ($response_code: expr, $message: expr) => {
        format!(
            "HTTP/1.1 {}\r\nContent-Length: {}\r\nContent-Type: text/plain\r\n\r\n{}\r\n",
            $response_code, $message.len() + 2, $message
        ).as_bytes()
    };
} 

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
struct Response {
    code: &'static str,
    message: Option<String>,
}

// This contains information about an HttpLine
struct LineBuffer {
    index: usize,
    size: usize,
    buffer: [u8; HttpRequest::MAX_LINE_LENGTH],
}

struct RequestBuffer {
    index: usize,
    size: usize,
    buffer: [u8; REQUEST_BUFFER_SIZE],
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
    fn to_http(&self) -> String {
        if let Some(message) = &self.message {
            return format!("HTTP/1.1 {}\r\nContent-Length: {}\r\nContent-Type: text/plain\r\n\r\n{}\r\n",
                &self.code, &message.len() + 2, &message // add 2 to account for trailing \r\n
            );
        } else {
            return format!("HTTP/1.1 {}\r\n", &self.code);
        }
    }

    // Sends an HTTP Response back to the client
    fn send(&self, client_stream: &mut TcpStream) {
        if let Err(error) = client_stream.write(self.to_http().as_bytes()) {
            println!("ERROR WHEN WRITING RESPONSE: {}", error.to_string());
        }
        if let Err(error) = client_stream.flush() {
            println!("ERROR WHEN SENDING RESPONSE: {}", error.to_string());
        }   
        
    }

    fn clone_with_message(&self, message: String) -> Response {
        return Response{code: self.code, message: Some(message)};
    }
}

// The body of an HTTP request will be stored as either a raw of bytes, or a
// struct called FormData which is used as the value for a MultiPart enum. More
// info about FormData can be found in its definition.
#[derive(Debug)]
enum Body {
    Single(Vec<u8>), // Raw byte stream. Used when receiving raw data. TODO: This may be able to take a fixed size array instead of a Vector.
    MultiPart(Vec<FormData>), // Struct with headers. Used when receiving multipart data.
    None
}

// The various types of PDF which are processed
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum PDFType {
    Unknown = 0,
    BatchWeight = 1,
    DeliveryTicket = 2,
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

// All relevant information in an HttpRequest
#[derive(Debug)]
struct HttpRequest { 
    method: String,
    location: String,
    query: HashMap<String, String>,
    version: String,
    headers: HashMap<String, String>,
    body: Body // Body is typically stored as a raw byte array
}

// TODO: Create a timezone for Atlantic Standard Time (UTC-4). This will prevent having to import chrono-tz package

// Stores PDF metadata
#[derive(Debug)]
struct PDFMetadata {
    pdf_type: PDFType,
    datetime: DateTime<Utc>, // could easily store this as a different datatype to save space
    customer: String,
    relative_path: String,
    doc_number: i32,
}

// Finds the index of the first predicate (the byte to be searched for) in an
// array of bytes. Searches over a specified range. Returns None if the
// predicate cannot be found.
fn u8_index_of(array: &[u8], predicate: u8, start_index: usize, end_index: usize) -> Option<usize> {
    let index = array[start_index .. end_index]
        .iter()
        .position(|&pred| pred == predicate);
    if index.is_none() {
        return None;
    } else {
        let index = index.unwrap() + start_index;
        return Some(index)
    }
}

// Finds the index of the first predicate (the array of bytes to be searched
// for) in an array of bytes. Searches over a specified range. Returns None if
// the predicate cannot be found.
fn u8_index_of_multi(array: &[u8], predicate: &[u8], start_index: usize, end_index: usize) -> Option<usize> {
    let index = array[start_index .. end_index]
        .windows(predicate.len())
        .position(|pred| pred == predicate);
    if index.is_none() {
        return None;
    } else {
        let index = index.unwrap() + start_index;
        return Some(index);
    }
}

impl Body {
    fn single(&self) -> Result<&Vec<u8>, &str> {
        if let Single(body) = self {
            return Ok(body);
        } else {
            return Err("Could not convert Body struct to byte array");
        }
    }
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
        
        debug_println!("Body parsed. Request up to this point: {:?}", request);

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

fn parse_location(location_line: &String) -> Result<(String, HashMap<String, String>), Response> {
    let values: Vec<&str> = location_line.split("?").collect();
    let location = values[0].to_string();
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

// The main loop for the webserver
fn main() {
    debug_println!("--Begin--  PDFs will be stored in: {}", PDFS_FILEPATH);
    
    // Create singletons which will be used throughout the program
    let listener = TcpListener::bind(LOCAL).expect("Aborting: Could not connect to server");
    let mut db = Client::connect(POSTGRES_ADDRESS, NoTls).unwrap();       
    let mut request_buffer = RequestBuffer{ size: 0, index: 0, buffer: [0; REQUEST_BUFFER_SIZE] }; 
    // request_buffer be used for every request. pre-allocating on stack.

    // Listens for a connection
    for stream in listener.incoming() {
        if let Err(error) = stream {
            debug_println!("TcpListener had an error when trying to listen for a TcpStream: {}", &error.to_string());
            continue;
        }
        let mut stream = stream.unwrap();

        let request = HttpRequest::parse(&mut stream, &mut request_buffer);
        if let Err(response) = request {
            response.send(&mut stream);
            continue;
        }
        
        let request = request.unwrap();
        let response: Response;
        debug_println!("Processing complete. Dispatching request");
        let response: Response = match (request.method.as_str(), request.location.as_str()) {
            ("GET", "/belgrade/documents/search")   => handle_query(&request, &mut db),
            ("GET", _)                              => handle_get(&request, &mut db),
            ("POST", "/api/belgrade/documents")     => handle_post(&request, &mut db),
            _                                       => NOT_IMPLEMENTED,
        };
        response.send(&mut stream);
    }
    
    // Should never reach this...
    if let Err(error) = db.close() {
        dbg!(error.to_string());
    }
}

// Handles an http query to the database
fn handle_query(request: &HttpRequest, db: &mut Client) -> Response {
    // Ensure all fields are present and decoded. Set defaults for empty strings
    let query = request.query.get("query");
    if query.is_none() {return BAD_REQUEST.clone_with_message("Query must have 'query' field".to_string());}
    let query = query.unwrap();
    let query = urlencoding::decode(query);
    if let Err(error) = query { return BAD_REQUEST.clone_with_message(format!("Could not decode query into UTF-8: {}", error.to_string())); }
    let query = query.unwrap().into_owned();
    if query.contains("\"") { return BAD_REQUEST.clone_with_message("queries cannot have the \" character in them.".to_string()); }

    let filter = request.query.get("filter");
    if filter.is_none() {return BAD_REQUEST.clone_with_message("Query must have 'filter' field".to_string());}
    let filter = filter.unwrap();
    let filter = urlencoding::decode(filter);
    if let Err(error) = filter { return BAD_REQUEST.clone_with_message(format!("Could not decode filter into UTF-8: {}", error.to_string())); }
    let filter = filter.unwrap().into_owned();
    if filter.contains("\"") { return BAD_REQUEST.clone_with_message("filters cannot have the \" character in them.".to_string()); }

    let from = request.query.get("from");
    if from.is_none() {return BAD_REQUEST.clone_with_message("Query must have 'from' field".to_string());}
    let from = from.unwrap();
    let from_datetime: DateTime<Utc>;
    if from.is_empty() {
        let thirty_days_ago = Utc::now().checked_sub_days(chrono::Days::new(30));
        if let Some(datetime) = thirty_days_ago {
            from_datetime = datetime;
        } else {
            return INTERNAL_SERVER_ERROR.clone_with_message("Server was unable to process 'from' datetime".to_string()); // Is this even possible?
        }
    } else {
        let datetime = GMTPlus4.datetime_from_str(&format!("{} 00:00:00", from), "%Y-%m-%d %H:%M:%S");
        match datetime {
            Ok(datetime) => from_datetime = datetime.with_timezone(&Utc),
            Err(error) => return BAD_REQUEST.clone_with_message(format!("'from' date was improperly formatted: {}", error.to_string())),
        }
    }

    let to = request.query.get("to");
    if to.is_none() {return BAD_REQUEST.clone_with_message("Query must have 'to' field".to_string());}
    let to = to.unwrap();
    let to_datetime: DateTime<Utc>;
    if to.is_empty() {
        to_datetime = Utc::now();
    } else {
        let datetime = GMTPlus4.datetime_from_str(&format!("{} 23:59:59", to), "%Y-%m-%d %H:%M:%S");
        match datetime {
            Ok(datetime) => to_datetime = datetime.with_timezone(&Utc),
            Err(error) => return BAD_REQUEST.clone_with_message(format!("'to' date was improperly formatted: {}", error.to_string())),
        }
    }

    if to_datetime < from_datetime {
        return BAD_REQUEST.clone_with_message("The 'from' is sooner than the 'to' date, or the from date does not exist".to_string());
    }

    debug_println!("Query: {} Filter: {} Processed datetimes --- From: {:?} To: {:?}", query, filter, from_datetime, to_datetime);
    
    // Everything has been extracted and processed. Ready for database query
    const BASE_REQUEST: &str = "SELECT pdf_datetime, pdf_type, pdf_num, customer, relative_path FROM pdfs WHERE pdf_datetime BETWEEN $1 AND $2";
    let full_query = match filter.as_str() {
        "Customer" => {
            format!("{} AND customer ILIKE '%{}%';", BASE_REQUEST, query)
        }
        "Delivery Ticket #" => {
            format!("{} AND pdf_type = 2 AND pdf_num = {};", BASE_REQUEST, query)
        },
        "Batch Weight #" => {
            format!("{} AND pdf_type = 1 AND pdf_num = {};", BASE_REQUEST, query)
        },
        _ => { 
            format!("{} AND relative_path ILIKE '%{}%';", BASE_REQUEST, query)
        }
    };
    
    // Execute query
    debug_println!("Query to be executed: {}", full_query);
    let rows = db.query(&full_query, &[&from_datetime, &to_datetime]);
    if let Err(error) = rows { return INTERNAL_SERVER_ERROR.clone_with_message(format!("Could not execute query on database: {}", error.to_string())); }
    let rows = rows.unwrap();
    
    // Create HTML table in response
    let mut table = "<table><tr><th>DateTime</th><th>Type</th><th>Num</th><th>Customer</th><th>Link</th></tr>".to_string();

    for row in rows {
        let datetime: DateTime<Utc> = row.get(0);
        let pdf_type: i32 = row.get(1);
        let pdf_num: i32 = row.get(2);
        let customer: &str = row.get(3);
        let relative_path: &str = row.get(4);
        debug_println!("Datetime: {:?}, PDF Type: {}, Num: {}, Customer: {} Path: {}", datetime, pdf_type, pdf_num, customer, relative_path);

        let table_row = format!("<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td><a href=\"{}{}\">Link</a></td></tr>",
                datetime.format("%Y-%b-%d %I:%M %p").to_string(), pdf_type, pdf_num, customer, PDFS_FILEPATH, relative_path);
        table.push_str(&table_row);
        // Generate HTML
    }
    table.push_str("</table>");
    debug_println!("{}", table);
    return NOT_IMPLEMENTED;
}

// Handles a get to belgrade
fn handle_get(request: &HttpRequest, db: &mut Client) -> Response {
    // 1. Verify Location [/belgrade/documents(/.*)]
    // 2. 
    return OK;
}

// Handles a POST Http request. This is typically where PDFs are received,
// analyzed, and sorted into the correct location. Returns a success or error
// message.
fn handle_post(request: &HttpRequest, db: &mut Client) -> Response {
    // According to the README, on a POST to /api/belgrade/documents, the
    // webserver is supposed to add a document to the database
    if !request.location.eq("/api/belgrade/documents") {
        return BAD_REQUEST.clone_with_message("POSTs can only be made to /api/belgrade/documents".to_owned());
    }

    // NOTE: Assumes that PDF is sent unmodified in Body. Currently, the minimum
    // required metadata for storing a PDF will be the date, customer, and
    // pdf-type (Delivery Ticket or Batch Weight).
    let pdf_as_bytes = request.body.single().unwrap();
    // let pdf_as_bytes = request.body.unwrap().single().unwrap();

    // Confirm that this file is indeed a PDF file
    if !pdf_as_bytes.starts_with(b"%PDF") {
        return BAD_REQUEST.clone_with_message("PDF File not detected: The PDF version header was not found".to_owned());
    }

    // Decide whether Batch Weight or Delivery Ticket or Undecidable.
    let pdf_type: PDFType;
    let id = [b"/Widths [", CR, LF, b"600 600 600 600 600 600 600 600 600"].concat();
    let id = id.as_slice();
    if let Some(result) = 
            pdf_as_bytes
                .windows(id.len())
                .find(|&pred| pred
                .eq(id)) {
        pdf_type = PDFType::BatchWeight;
    } else {
        let id = [b"/Widths [", CR, LF, b"277 333 474 556 556 889 722 237 333"].concat();
        let id = id.as_slice();
        if let Some(result) = 
                pdf_as_bytes
                    .windows(id.len())
                    .find(|&pred| pred
                    .eq(id)) {
            pdf_type = PDFType::DeliveryTicket; 
        } else {
            pdf_type = PDFType::Unknown;
        }
    } 

    // Deflate and retrieve date and customer. FIXME: This currently breaks when
    // ticket requests are received to quickly. I am not sure why this may
    // occur, but I believe this may be because when testing with curl, there is
    // an attempt to have a keep alive or something when rapidly sending large
    // requests, and this is not implemented. More research needs to be done,
    // but this won't be a problem so long as the server responds saying that
    // the request did not go through. 
    let mut date = String::new();
    let mut customer = String::new();
    let mut doc_number = 0;
    let mut time = String::new();
    let LENGTH_PREFIX = b"<</Length ";
    let mut i = 0;
    while let Some(flate_header) = u8_index_of_multi(pdf_as_bytes.as_slice(), LENGTH_PREFIX, i, pdf_as_bytes.len()) {
        if pdf_type == PDFType::Unknown {break;}
        // Get line which sets up Flate decode and extract the length from it
        let length_start_index = flate_header + LENGTH_PREFIX.len();
        let length_end_index = u8_index_of(&pdf_as_bytes, b'/', length_start_index, pdf_as_bytes.len()).unwrap();
        let length = pdf_as_bytes[length_start_index..length_end_index].to_vec();
        let digits: Vec<u8> = 
            length
                .iter()
                .map(|&c| c - 48)
                .collect();
        let length = digits.from_digits() as usize;
        let stream_start_index = u8_index_of(&pdf_as_bytes, CR[0], length_end_index, pdf_as_bytes.len());
        if stream_start_index == None {
            return BAD_REQUEST.clone_with_message("Could not find the start of the Flate Encoded Stream. FlateStream should be prefaced by a CRLF pattern, which was not detected. This can occur when the data is not sent as a binary file.".to_string());
        }
        let stream_start_index = stream_start_index.unwrap() + 2; //NOTE: The unwrap is safe, the +2 is not
        let stream_end_index = stream_start_index + length;
        i = stream_end_index;
        let stream = &pdf_as_bytes[stream_start_index..stream_end_index];
        let mut output_buffer = String::new();
        debug_println!("=======CHECKPOINT=========");
        zlib::Decoder::new(stream).read_to_string(&mut output_buffer);
        // debug_println!("zlib output: {:?}", &output_buffer);
        // debug_println!("Stream start: {} End: {} Size: {} Length: {}", stream_start_index, stream_end_index, stream.len(), length);
        // FIXME: This will break when a key, value pair is along a boundary
        let DATE_PREFIX = if pdf_type == PDFType::DeliveryTicket {"Tf 480.8 680 Td ("} else {"BT 94 734 Td ("}; // NOTE: This should be a const, and is used improperly
        let date_pos = output_buffer.find(DATE_PREFIX);
        if let Some(mut date_pos) = date_pos {
            date_pos += DATE_PREFIX.len();
            let date_end_pos = u8_index_of_multi(&output_buffer.as_bytes(), b")Tj", date_pos, output_buffer.len()).unwrap();
            date = output_buffer[date_pos..date_end_pos].to_string(); //NOTE: DANGEROUS 
        }

        let DOC_NUM_PREFIX = if pdf_type == PDFType::DeliveryTicket {"Tf 480.8 668.8 Td ("} else {"BT 353.2 710 Td ("};
        let doc_num_pos = output_buffer.find(DOC_NUM_PREFIX);
        if let Some(mut doc_num_pos) = doc_num_pos {
            doc_num_pos += DOC_NUM_PREFIX.len();
            let doc_num_end_pos = u8_index_of_multi(&output_buffer.as_bytes(), b")Tj", doc_num_pos, output_buffer.len()).unwrap();
            doc_number = output_buffer[doc_num_pos..doc_num_end_pos].parse().unwrap(); //NOTE: DANGEROUS 
        }

        let TIME_PREFIX = if pdf_type == PDFType::DeliveryTicket {""} else {"BT 353.2 686 Td ("}; // NOTE: This should be a const, and is used improperly
        let time_pos = output_buffer.find(TIME_PREFIX);
        if time_pos != None && pdf_type == PDFType::BatchWeight {
            let time_pos = time_pos.unwrap() + TIME_PREFIX.len();
            let time_end_pos = u8_index_of_multi(&output_buffer.as_bytes(), b")Tj", time_pos, output_buffer.len()).unwrap();
            time = output_buffer[time_pos..time_end_pos].to_string(); //NOTE: DANGEROUS 
        }
        
        let CUSTOMER_PREFIX = if pdf_type == PDFType::DeliveryTicket {"Tf 27.2 524.8 Td ("} else {"BT 94 722 Td ("};
        let customer_pos = output_buffer.find(CUSTOMER_PREFIX);
        if let Some(mut customer_pos) = customer_pos {
            customer_pos += CUSTOMER_PREFIX.len();
            let customer_end_pos = u8_index_of_multi(&output_buffer.as_bytes(), b")Tj", customer_pos, output_buffer.len()).unwrap();
            customer = output_buffer[customer_pos..customer_end_pos].to_string();
        } 
    }

    // Parse string date formats into Chrono (ISO 8601) date formats. Delivery
    // Tickets all currently have their DateTimes set to 12 noon EST, since
    // extracting their datetimes is hard and cannot be done yet. More info in
    // issue 3 on GitHub.
    let mut datetime = Utc.timestamp_nanos(0);
    if pdf_type == PDFType::BatchWeight {
        let combined = format!("{} {}", &date, &time);
        debug_println!("date: {}, time: {}, combined: {}", &date, &time, &combined);
        datetime = GMTPlus4.datetime_from_str(&combined, "%e-%b-%Y %I:%M:%S %p").unwrap().with_timezone(&Utc);
    } else if pdf_type == PDFType::DeliveryTicket {
        let combined = format!("{} 12:00:00", &date);
        debug_println!("date: {}", &combined);
        datetime = GMTPlus4.datetime_from_str(&combined, "%d/%m/%Y %H:%M:%S").unwrap().with_timezone(&Utc); 
    } else {
        datetime = Utc.with_ymd_and_hms(1970, 1, 1, 12, 0, 0).unwrap();
    }

    // Generate a relative filepath (including filename) of the PDF. Files will be sorted in folders by years and then months
    let result_row = db.query(
        "SELECT COUNT(*) FROM pdfs WHERE CAST(pdf_datetime as DATE) = $1 AND pdf_num = $2;",
        &[&datetime.date_naive(), &(doc_number as i32)] // NOTE: This cast is redundant, but VSCode thinks it is an error without it. Rust does not. It compiles and runs and passes testcases.
    );
    if let Err(e) = result_row {
        return INTERNAL_SERVER_ERROR.clone_with_message(e.to_string());
    }
    let result_row = result_row.unwrap();
    let num_entries: i64 = result_row[0].get(0);
    
    let duplicate = if num_entries == 0 {String::new()} else {format!("_{}",num_entries.to_string())}; // There should only ever be one entry for this, but should a duplicate arise this handles it.
    let type_initials = if pdf_type == PDFType::DeliveryTicket {"DT"} else if pdf_type == PDFType::BatchWeight {"BW"} else {"ZZ"};
    let relative_filepath = format!("{}_{}_{}{}{}.pdf",datetime.format("%Y/%b/%d").to_string(), customer, type_initials, doc_number, duplicate); // eg. 2022/Aug/7_John Doe_DT154.pdf
    
    
    let pdf_metadata = PDFMetadata { 
        datetime:       datetime, // NOTE: As of Dec 21 2022, this date uses a different format in Batch Weights vs Delivery Tickets
        pdf_type:       pdf_type,
        customer :      customer,
        relative_path:  relative_filepath,
        doc_number:     doc_number,
    };
    
    // Place the PDF file into the correct place into the filesystem
    {
        let path_string = format!("{}{}", PDFS_FILEPATH, &pdf_metadata.relative_path);
        let path = Path::new(&path_string);
        let prefix = path.parent().unwrap(); // path without final component
        debug_println!("Prefix: {:?}", prefix);
        fs::create_dir_all(prefix).unwrap();
        let mut pdf_file = File::create(&path_string).unwrap();
        pdf_file.write_all(pdf_as_bytes.as_slice()).unwrap();
    }
    
    debug_println!("METADATA: {:#?}", pdf_metadata);

    // Store PDF into Database
    db.query(concat!("INSERT INTO pdfs (pdf_type, pdf_num, pdf_datetime, customer, relative_path)",
            "VALUES ($1, $2, $3, $4, $5);"),
            &[&(pdf_metadata.pdf_type as i32),
            &pdf_metadata.doc_number,
            &datetime, 
            &pdf_metadata.customer,
            &pdf_metadata.relative_path]
    ).unwrap();


    // This is where the PDF should be parsed
    return CREATED.clone_with_message("PDF received and stored on server succesfully".to_owned());
}