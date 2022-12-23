use std::{env, fs};
use std::fs::File;
use std::io::{prelude::*, BufReader};
use std::net::{TcpListener, TcpStream};
use std::collections::HashMap;
use std::path::Path;
use chrono::{Utc, TimeZone, DateTime};
use chrono_tz::Etc::{GMTMinus4, GMTPlus4};
use compress::zlib;
use num_digitize::FromDigits;
use postgres::{Client, NoTls};
use crate::Body::{Single, MultiPart};

// RUST WEBSERVER CONSTANTS
const KB: usize = 1024;
const MB: usize = KB * 1024;
const GB: usize = MB * 1024;
const LOCAL: &str = "127.0.0.1:7878";
const IPV4: &str = "45.77.158.123:7878";
const IPV6: &str = "[2001:19f0:5:5996&:5400:4ff:fe02:3d3e]:7878";
const MAX_REQ: usize = (256 * KB) - 1;
const CR: &[u8] = &[13 as u8];
const LF: &[u8] = &[10 as u8];
const PDFS_FILEPATH: &str = r"C:/dev/rmc/site/belgrade/documents/";

// DATABASE CONSTANTS


// NOTE: Receiving multipart/formdata is not currently supported.

/*
 * TODO
 * 
 * 1. Design a good way of organising these files so that they can be searched for on the website
 *    (Database of file names? Maybe. Sounds redundant. May be necessary).
 * 2. For now, this program will load the ENTIRE http request into memory. This drastically limits
 *      the size of requests you can process. This is okay since the size of requests are known
 *      beforehand. (The largest will be running PUT with a PDF file a few dozen KB in length). But
 *      I'd like to handle larger payloads using streams, buffers, file io, etc.
 * 3. TODO: Error, a crash can happen on a line with TODO: ERROR relating to headers without values
 * 4. TODO: Either read files in 8KB increments, or do not allow any HTTP requests larger than 8KB. 
 *      This prevents eating up all memory. It must be limited somehow to avoid a random 1GB PDF file 
 *      eating up all the memory.
 */

// Implement parsing of multipart/formdata 
// Implement extraction of PDFs
// Put them into a website
// Let you search by them
// Hugo tutorial

// Formats an HTTP response to an array of bytes.
macro_rules! response {
    ($response_code: expr, $message: expr) => {
        format!(
            "HTTP/1.1 {}\r\nContent-Length: {}\r\nContent-Type: text/plain\r\n\r\n{}\r\n",
            $response_code, $message.len() + 2, $message
        ).as_bytes()
    };
} 

// Println only in debug
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

// The body of an HTTP request will be stored as either a raw of bytes, or a
// struct called FormData which is used as the value for a MultiPart enum. More
// info about FormData can be found in its definition.
#[derive(Debug)]
enum Body {
    Single(Vec<u8>), // Raw byte stream. Used when receiving raw data. TODO: This may be able to take a fixed size array instead of a Vector.
    MultiPart(Vec<FormData>) // Struct with headers. Used when receiving multipart data.
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
struct HttpRequest {
    method: String,
    location: String,
    version: String,
    headers: HashMap<String, String>,
    body: Body // Body is typically stored as a raw byte array
}

// Create a timezone for Atlantic Standard Time (UTC-4)


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
    // Returns a new HttpRequest struct from a well formatted HttpRequest as
    // bytes.
    // FIXME: Function assumes that HTTP requests are properly formed. 
    // NOTE: 'a is a lifetime indicator. It indicates that the lifetime of all
    // references with the generic type &'a are linked together. If one variable
    // is freed, the other will also be freed. (In this case, if
    // request_as_bytes no longer exists, then the error string will also not
    // exist).
    // TODO: I do not enjoy this functionality, but it is necessary (I believe)
    // in order to use the response! macro to generate responses at compile
    // time, when in reality, the http response code is not known until runtime.
    // Before this code goes live, I need to think about this more. This may be
    // able to be fixed without too much effort by using the ? symbol instead of
    // .unwrap().
    fn new<'a>(request_as_bytes: &'a[u8], mut stream: &TcpStream) -> Result<HttpRequest, &'a  str> {
        // Request line
        let request_size = request_as_bytes.len();
        if request_size == 0 {
            return Err("Http Request received with length 0");
        }
        let method_end = u8_index_of(request_as_bytes, b' ', 0, request_size).unwrap();
        let method = String::from_utf8_lossy(&request_as_bytes[..method_end]).into_owned();

        let location_end = u8_index_of(request_as_bytes, b' ', method_end + 1,  request_size);
        let request_line_end = u8_index_of(request_as_bytes, b'\r', method_end + 1, request_size)
            .expect("HTML is malformed");
        let location: String;
        let version: String;
        match location_end {
            None => {
                location = String::from_utf8_lossy(&request_as_bytes[method_end+1..request_line_end]).into_owned();
                version = format!("HTTP/1.0");
            },
            Some(location_end) => {
                location = String::from_utf8_lossy(&request_as_bytes[method_end+1..location_end]).into_owned();
                version = String::from_utf8_lossy(&request_as_bytes[location_end+1..request_line_end]).into_owned()
            }
        }

        // Headers
        let mut header_start = request_line_end + 2;
        match (version.as_str(), request_as_bytes[header_start] == b'\0') {
            ("HTTP/1.0", true) => {
                return Ok(
                    HttpRequest {
                        method: method,
                        location: location,
                        version: version,
                        headers: HashMap::new(),
                        body: Single(Vec::new()),
                    }
                );
            },
            (_, true) => {
                return Err("Request malformed. HTTP versions greated that 1.0 must contain a 'Host' header");
            },
            (_, false) => ()
        }
        let mut header_end = u8_index_of(request_as_bytes, b'\r', header_start, request_size)
            .expect("HTTP request is malformed. Each header must end with CRLF");
        let head_end = u8_index_of_multi(request_as_bytes, b"\r\n\r\n", header_end, request_size)
            .expect("HTTP request is malformed. Head must end with CRLF CRLF");
        let mut headers: HashMap<String, String> = HashMap::new();
        while header_start < head_end {
            let delim_pos = u8_index_of(request_as_bytes, b':', header_start, header_end)
                .expect("HTTP request is malformed. Every header must contain a key and a value separated by ':'");
            let header_key = &request_as_bytes[header_start..delim_pos];
            let mut header_value = &request_as_bytes[delim_pos+1..header_end]; //TODO: ERROR: This can break if there is no value, because it will save a 0 length byte array
            while header_value[0] == b' ' {
                header_value = &header_value[1..];
            }
            let header_key = String::from_utf8_lossy(header_key).to_ascii_lowercase();
            let header_value = String::from_utf8_lossy(header_value).into_owned();
            headers.insert(header_key, header_value);

            header_start = header_end + 2;
            header_end = u8_index_of(request_as_bytes, b'\r', header_start, head_end + 4)
                .expect("I don't know what went wrong");
        }

        // Get Body
        let body_start = head_end + 4;
        let body_in_buffer = body_start < request_size;
        let content_length = headers.get("content-length");
        let content_type = headers.get("content-type");
        //TODO: The below if statements are disgusting
        if let Some(content_type) = content_type {
            if content_type.starts_with("multipart") {
                return Err("Multipart data is not allowed at this time");
                /* Not gonna bother implementing multipart rn
                lazy_static! {
                    static ref RE_BOUNDARY: Regex = Regex::new("boundary=[\"]?(.*)[\"|;|\r]").unwrap();
                }
                let boundary = &RE_BOUNDARY.captures(&content_type).unwrap()[1];
                let boundary_as_bytes = boundary.as_bytes();
                let mut body = Vec::new();
                match (&content_length, &first_byte_of_body) {
                    (Some(length), b'\0') => {
                        if let Ok(length) = length.parse::<usize>() {
                            body.resize(length, b'\0');
                            let mut reader = BufReader::new(stream);
                            reader.read_exact(&mut body).unwrap();
                        } // The sender (client) should be informed of what's going on in the else case here. The request has a content length header that is not made up of entirely numbers
                    },
                    // The next two options are caused when there is no Content-Length header. IMO this is undefined behavior, and we should let the client know something is wrong
                    (None, b'\0') => {
                        // This may as well be an error
                        return Ok(HttpRequest{method, location, version, headers, body: MultiPart(Vec::new())})
                    },
                    (_, _) => {
                        body = request_as_bytes[body_start..].to_vec();
                    }
                }
                let mut after_boundary = boundary.len();
                let forms: Vec<FormData> = Vec::new();
                while &body[after_boundary..after_boundary+4] != b"--\r\n" {

                }
                return Ok(HttpRequest{method, location, version, headers, body: MultiPart(Vec::new())})
                */
            } else {
                let mut body = Vec::new();
                match (content_length, body_in_buffer) {
                    (Some(length), false) => {
                        debug_println!("Hopeful path");
                        if let Ok(length) = length.parse::<usize>() {
                            body.resize(length, b'\0');
                            let mut bytes_read = stream.read(&mut body).unwrap(); // A connection can be forcibly closed by the client, at this point the program will crash
                            debug_println!("bytes_read: {} body len: {}", bytes_read, body.len());
                            while bytes_read < body.len() { // FIXME: This is bugged. When content is passed in multiple buffer, the second read overwrites the buffer of the first. 
                                bytes_read += stream.read(&mut body).unwrap();
                                debug_println!("bytes_read: {} body len: {}", bytes_read, body.len());
                            }
                            debug_println!("got here");
                        } // The sender (client) should be informed of what's going on here. The request has a content length header that is not made up of entirely numbers
                    },
                    // The next two options are caused when there is no
                    // Content-Length header. For now this will be undefined
                    // behaviour, and the webserver will reject all requests
                    // without one. The webserver lets the client know that the
                    // request was invalid.
                    (None, false) => (),
                    (_, true) => {
                        debug_println!("Body was in buffer");
                        body = request_as_bytes[body_start..].to_vec();
                    }
                };

                // Option 1
                // let mut body: Vec<u8> = Vec::new();
                // match (&content_length, body_in_buffer) {
                //     (Some(length), false) => {
                //         if let Ok(length) = length.parse::<usize>() {
                //             body.resize(length, b'\0');
                //             let mut reader = BufReader::new(stream);
                //             reader.read_exact(&mut body).unwrap();
                //         } // The sender (client) should be informed of what's going on here. The request has a content length header that is not made up of entirely numbers
                //     },
                //     // The next two options are caused when there is no Content-Length header. IMO this is undefined behavior, and we should let the client know something is wrong
                //     (None, false) => (),
                //     (_, true) => {
                //         body = request_as_bytes[body_start..].to_vec();
                //     }
                // };
                
                return Ok( HttpRequest {method, location, version, headers, body: Single(body)} );
            }
        } else {
            let mut body: Vec<u8> = Vec::new();
            match (&content_length, body_in_buffer) {
                (Some(length), _) => {
                    if let Ok(length) = length.parse::<usize>() {
                        body.resize(length, b'\0');
                        let mut reader = BufReader::new(stream);
                        reader.read_exact(&mut body).unwrap();
                    } // The sender (client) should be informed of what's going on here. The request has a content length header that is not made up of entirely numbers
                },
                // The next two options are caused when there is no Content-Length header. IMO this is undefined behavior, and we should let the client know something is wrong
                (None, false) => (),
                (None, true) => {
                    body = request_as_bytes[body_start..].to_vec();
                }
            };
            
            return Ok( HttpRequest {method, location, version, headers, body: Single(body)} );
        }
    }
}

// The main loop for the webserver
fn main() {

    debug_println!("Working Directory: {:?}\nExecutable Directory: {:?}", env::current_dir().unwrap(), env::current_exe().unwrap());
    
    // Create singletons which will be used throughout the program
    let listener = TcpListener::bind(LOCAL).expect("Could not connect to server");
    let mut db = Client::connect("postgresql://nate:testpasswd@localhost/rmc", NoTls).unwrap();
        
    // Listens for a connection
    for stream in listener.incoming() {
        
        // Max request (for now)
        let mut raw_request = [b'\0'; MAX_REQ]; // Max request size
        let mut stream = stream.unwrap();
        let request_size = stream.read(&mut raw_request).unwrap();
        debug_println!("Request size: {}", request_size);

        // FIXME: This part of code will never be reached, since strea.read()
        // will not overflow the buffer. What really should happen in this whole
        // program is an attempt at buffered reading. This would also make it so
        // that the memory footprint would be constant
        if request_size > MAX_REQ { 
            stream.write(
                response!("400 Bad Request", "Http Request is larger than max allowed request (64KB).")
            ).unwrap();
            stream.flush().unwrap();
            continue;
        } 

        let request = HttpRequest::new(&raw_request[..request_size], &stream);
        if let Err(error) = request {
            stream.write(response!("400 Bad Request", error)).unwrap();
            stream.flush().unwrap();
            continue;
        } 
        
        let request = request.unwrap();
        debug_println!("Body size: {} Headers: {:#?}", request.body.single().unwrap().len(), request.headers);
        // debug_println!("\nMethod: {:#?}\nLocation: {:#?}\nVersion: {:#?}\nHeaders: {:#?}\nBody: {:?}\n", 
        //     request.method, request.location, request.version, request.headers, request.body);

        // Parse request
        match request.method.as_str() {
            "GET" => {
                stream.write(response!("200 OK", "GET REQUEST RECEIVED")).unwrap()
            }
            "POST" => {
                debug_println!("Attemping to process post");
                if let Err(error) = handle_post(&request, &mut db) {
                    debug_println!("ERROR IN HANDLING REQUEST: {}", error);
                    stream.write(response!("400 Bad Request", error));
                    stream.flush().unwrap();
                    continue;
                }
                
                stream.write( response!("200 OK", "POST received")).unwrap()
            }
            _=> stream.write(response!("400 Bad Request", [request.method.as_str(), "is not supported yet"].join(" "))).unwrap() 
        };

        stream.flush().unwrap();
    }

    db.close().unwrap(); 
}

// Handles a POST Http request. This is typically where PDFs are received,
// analyzed, and sorted into the correct location. Returns a success or error
// message.
fn handle_post(request: &HttpRequest, db: &mut Client) -> Result<String, String> {
    // According to the README, on a POST to /api/belgrade/documents, the
    // webserver is supposed to add a document to the database
    if !request.location.eq("/api/belgrade/documents") {
        return Err("POSTs can only be made to /api/belgrade/documents".to_owned());
    }

    // NOTE: Assumes that PDF is sent unmodified in Body. Currently, the minimum
    // required metadata for storing a PDF will be the date, customer, and
    // pdf-type (Delivery Ticket or Batch Weight).
    let pdf_as_bytes = request.body.single().unwrap();
    
    // Confirm that this file is indeed a PDF file
    if !pdf_as_bytes.starts_with(b"%PDF") {
        return Err("PDF File not detected: The PDF version header was not found".to_owned());
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
    while let Some(flate_header) = u8_index_of_multi(pdf_as_bytes, LENGTH_PREFIX, i, pdf_as_bytes.len()) {
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
            return Err("Could not find the start of the Flate Encoded Stream. Stream should be prefaced by a CRLF pattern, which was not detected. This can occur when the data is not sent as a binary file.".to_owned());
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
        return Err(e.to_string());
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
        pdf_file.write_all(pdf_as_bytes).unwrap();
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
    return Ok("PDF received and stored on server succesfully".to_owned());
}