use std::fs::{File, self};
use std::io::{prelude::*};
use std::net::TcpListener;
use std::path::Path;
use chrono::{Utc, TimeZone, DateTime};
use chrono_tz::Etc::GMTPlus4;
use compress::zlib;
use num_digitize::FromDigits;
use postgres::{Client, NoTls};
mod http;
use crate::http::*;


// RUST WEBSERVER CONSTANTS
const REQUEST_BUFFER_SIZE: usize = 4096;
const LOCAL: &str = "127.0.0.1:7878";
const IPV4: &str = "45.77.158.123:7878";
const IPV6: &str = "[2001:19f0:5:5996&:5400:4ff:fe02:3d3e]:7878";
const ROOT_DIR: &str ="/home/nate/code/rmc/site";
const POSTGRES_ADDRESS: &str = "postgresql://nate:testpasswd@localhost/rmc";
const CR: &[u8] = &[13 as u8];
const LF: &[u8] = &[10 as u8];
// const ROOT_DIR: &str = "https://redimixdominica.com/belgrade/documents/";

// [1] The current request buffer size is 4KB, the pagesize on the computer I'm
// running the server on (and most Linux servers as of 2022 Dec). In theory,
// memory aligned data speeds up data access by keeping the cache hot, and makes
// the maximal use of memory, but I can't help but feel that I'm missing
// something. More research and testing needs to be done to find the optimal
// request buffer size.

// [2] As of 2022 Dec, receiving multipart/formdata is not supported, and is
// very low priority. All data must be sent as a binary stream in the body of a
// request.


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



// The various types of PDF which are processed
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum PDFType {
    Unknown = 0,
    BatchWeight = 1,
    DeliveryTicket = 2,
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

// The main loop for the webserver
fn main() {
    debug_println!("--Begin--  Root dir is set at: {}", ROOT_DIR);
    
    // Create singletons which will be used throughout the program
    let listener = TcpListener::bind(LOCAL).expect("Aborting: Could not connect to server");
    let mut db = Client::connect(POSTGRES_ADDRESS, NoTls).unwrap();       
    let mut request_buffer = RequestBuffer::new(); 
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
            response.send(&mut stream, "text/plain");
            continue;
        }
        
        let request = request.unwrap();
        let response: Response;
        debug_println!("Processing complete. Dispatching request");
        let response: Response = match (request.method.as_str(), request.location.as_str()) {
            ("GET", "/belgrade/documents/search")   => handle_query(&request, &mut db),
            ("GET", "/styles.css")                  => handle_styles(&request),
            ("GET", "/belgrade/documents/"_)        => get_document_search(&request),
            ("POST", "/api/belgrade/documents")     => handle_post(&request, &mut db),
            _                                       => NOT_IMPLEMENTED,
        };
        match request.location.as_str() {
            ("/styles.css")                 => response.send(&mut stream, "text/css"),
            ("/belgrade/documents/search")  => response.send(&mut stream, "text/html"),
            _                               => response.send(&mut stream, "text/plain"),
        }
    }
    
    // Should never reach this...
    if let Err(error) = db.close() {
        dbg!(error.to_string());
    }
}

// Returns the stylesheet
fn handle_styles (request: &HttpRequest) -> Response {
    let mut stylesheet: Vec<u8> = vec![0; 4096];
    let mut file = File::open(format!("{}/styles.css", ROOT_DIR));
    if let Err(error) = file {return INTERNAL_SERVER_ERROR.clone_with_message("Could not open the styles.css file".to_owned()); }
    let mut file = file.unwrap();
    let bytes_read = file.read(&mut stylesheet);
    if let Err(error) = bytes_read {return INTERNAL_SERVER_ERROR.clone_with_message("Could not read from the index.html file".to_owned()); }
    return OK.clone_with_message(String::from_utf8(stylesheet).unwrap());
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
    let entries = rows.len();
    let mut table = format!("<p>Found {} entries</p><table><tr><th>DateTime</th><th>Type</th><th>Num</th><th>Customer</th><th>Link</th></tr>", entries);
    for row in rows {
        let datetime: DateTime<Utc> = row.get(0);
        let pdf_type: i32 = row.get(1);
        let pdf_type = match pdf_type {
            1 => {"BW"},
            2 => {"DT"},
            _ => {"N/A"},
        };
        let pdf_num: i32 = row.get(2);
        let customer: &str = row.get(3);
        let relative_path: &str = row.get(4);
        // debug_println!("Datetime: {:?}, PDF Type: {}, Num: {}, Customer: {} Path: {}", datetime, pdf_type, pdf_num, customer, relative_path);

        let table_row = format!("<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td><a href=\"{}/belgrade/documents/{}\">Link</a></td></tr>",
                datetime.format("%Y-%b-%d %I:%M %p").to_string(), pdf_type, pdf_num, customer, ROOT_DIR, relative_path);
        table.push_str(&table_row);
    }
    table.push_str("</table>");

    // Read from source html file and return appended file to client
    let mut index: Vec<u8> = vec![0;2048];
    let mut file = File::open(format!("{}/belgrade/documents/index.html", ROOT_DIR));
    if let Err(error) = file {return INTERNAL_SERVER_ERROR.clone_with_message("Could not open the index.html file".to_owned()); }
    let mut file = file.unwrap();
    let bytes_read = file.read(&mut index);
    if let Err(error) = bytes_read {return INTERNAL_SERVER_ERROR.clone_with_message("Could not read from the index.html file".to_owned()); }
    let bytes_read = bytes_read.unwrap();
    let end = index.windows(7).position(|x| x == b"</form>");
    if end.is_none() {return INTERNAL_SERVER_ERROR.clone_with_message("Could not parse the server's own index.html file".to_string()); }
    let mut end = end.unwrap();
    end += 7;

    // Overwrite
    let mut table = table.as_bytes().to_vec();
    let mut index = index[..end].to_vec();
    index.append(&mut table);
    index.append(&mut b"</body>".to_vec());

    return OK.clone_with_message(String::from_utf8(index).unwrap());
}

// Handles a get to belgrade
fn get_document_search(request: &HttpRequest) -> Response {
    let mut index: Vec<u8> = vec![0;2048];
    let mut file = File::open(format!("{}/belgrade/documents/index.html", ROOT_DIR));
    if let Err(error) = file {return INTERNAL_SERVER_ERROR.clone_with_message("Could not open the index.html file".to_owned()); }
    let mut file = file.unwrap();
    let bytes_read = file.read(&mut index);
    if let Err(error) = bytes_read {return INTERNAL_SERVER_ERROR.clone_with_message("Could not read from the index.html file".to_owned()); }
    let bytes_read = bytes_read.unwrap();
    return OK.clone_with_message(String::from_utf8(index).unwrap());
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
        let path_string = format!("{}{}{}", ROOT_DIR, "/belgrade/documents/", &pdf_metadata.relative_path);
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