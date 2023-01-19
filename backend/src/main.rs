use std::collections::HashMap;
use std::fs::{File, self};
use std::io::{prelude::*, BufReader};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use base64::{Engine as _, engine::general_purpose};
use chrono::{Utc, TimeZone, DateTime};
use chrono_tz::Etc::GMTPlus4;
use compress::zlib;
use crc::Crc;
use num_digitize::FromDigits;
use postgres::{Client, NoTls, GenericClient};
use pwhash::sha512_crypt;
mod http;
mod config_parser;
use crate::http::*;


// RUST WEBSERVER CONSTANTS
const CONFIG_PATH: &str = "/home/nate/code/rmc/backend/data/config.ini";
const CR: &[u8] = &[13 as u8];
const LF: &[u8] = &[10 as u8];

// [1] The current request buffer size is 4KB, the pagesize on the computer I'm
// running the server on (and most Linux servers as of 2022 Dec). In theory,
// memory aligned data speeds up data access by keeping the cache hot, and makes
// the maximal use of memory, but I can't help but feel that I'm missing
// something. More research and testing needs to be done to find the optimal
// request buffer size.

// [2] As of 2022 Dec, receiving multipart/formdata is not supported, and is
// very low priority. All data must be sent as a binary stream in the body of a
// request.

#[derive(Debug)]
struct Config {
    content_root_dir: String,
    ip: String,
    domain_name: String,
    postgres_address: String,
}

impl Config {
    // This is critical path, but only runs once at the very start of the code
    // so this code SHOULD panic is something goes wrong
    pub fn parse_from(filepath: &str) -> Config {
        let values = config_parser::parse(filepath).unwrap();
        
        let domain_name = values.get("domain_name").unwrap();
        let content_root_dir = values.get("content_root_dir").unwrap();
        let postgres_address = values.get("postgres_address").unwrap();
        let ip = values.get("ip").unwrap();

        return Config {
            content_root_dir: content_root_dir.to_string(), 
            ip: ip.to_string(),
            domain_name: domain_name.to_string(),
            postgres_address: postgres_address.to_string()
        };
    }
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

// Macro which returns an error value to a function if it exists
macro_rules! unwrap_either { // TODO: This could use a better name
    // match something(q,r,t,6,7,8) etc
    // compiler extracts function name and arguments. It injects the values in respective varibles.
        ($a:ident)=>{
           {
            match $a {
                Ok(value)=>value,
                Err(err)=>{
                    return err;
                }
            }
            }
        };
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
    crc32_checksum: u32,
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
    let config = Config::parse_from(CONFIG_PATH);
    debug_println!("Values read from config: {:#?}", config);
    
    // Create singletons which will be used throughout the program
    let listener = TcpListener::bind(config.ip).expect("Aborting: Could not connect to server");
    let mut db = Client::connect(&config.postgres_address, NoTls).unwrap();       
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
            response.send(&mut stream);
            continue;
        }
        
        let request = request.unwrap();
        debug_println!("Processing complete. Dispatching request");
        // special checks for any documents
        if request.method.eq("GET") && request.location.starts_with("/belgrade/documents/") && request.location.ne("/belgrade/documents/search") {
            let response;
            if let Err(redirect_response) = check_authentication(&request.location, &request.query, request.headers.get("Cookie"), &config.domain_name, &mut db) {
                response = redirect_response;
            } else {
                response = get_pdf(&request.location, &config.content_root_dir); 
            }
            response.send(&mut stream);
            continue;
        }

        let response: Response = match (request.method.as_str(), request.location.as_str()) {
            ("GET", "/belgrade/documents/search")       => handle_query(&request, &mut db, &request.query, &config.content_root_dir, &config.domain_name),
            ("GET", "/belgrade/documents")              => get_document_search(&request.location, &request.query, request.headers.get("Cookie"), &config.domain_name, &config.content_root_dir, &mut db),
            ("GET", "/css/styles.css")                  => get_styles(&request.location, &config.content_root_dir),
            ("GET", "/login")                           => get_login(&config.content_root_dir),
            ("GET", "/api/user/login")                  => check_login(&request.query, &request.headers.get("Referer"), &config.domain_name, &mut db),
            ("GET", "/api/user/change-password")        => todo!("Need to do this"), //change_password(&request.query, &request.headers, &mut db),
            ("GET", "/api/belgrade/documents/exists")   => document_exists(&request.query, &mut db),
            ("POST","/api/belgrade/documents/")          => handle_post(&request, &mut db, &config.content_root_dir),
            _                                           => NOT_IMPLEMENTED,
        };

        response.send(&mut stream);
    }
    
    // Should never reach this...
    if let Err(error) = db.close() {
        dbg!(error.to_string());
    }
}

// Reads all bytes from a file. Returns a response if it was unable to.
fn read_all_bytes(filepath: &str) -> Result<Vec<u8>, Response> {
    let file = File::open(filepath);
    if let Err(error) = file {
        return Err(INTERNAL_SERVER_ERROR.clone_with_message(format!("Could not open the requested file: {}", error.to_string())));
    }
    let mut file = file.unwrap();
    let mut bytes = Vec::new();
    let bytes_read = file.read_to_end(&mut bytes);
    if let Err(error) = bytes_read {
        return Err(INTERNAL_SERVER_ERROR.clone_with_message(format!("Could not read from the requested file: {}", error.to_string()))); 
    }
    return Ok(bytes);
}

// Return the login page. Save the referer in the URL 
fn get_login(content_root_dir: &str) -> Response {
    let login = read_all_bytes(&format!("{}{}", content_root_dir, "/login.html"));
    let login = unwrap_either!(login);  
    
    let mut response = OK;
    response.add_header("content-type", HTML.to_string());
    response.add_message(login);
    return response;
}

// A user has just submitted a form with his credentials. Perform the
// cryptographic hashing and match it to data stored in the server. If the user
// is who they say they are, give them a cookie and return them to the page they
// tried to access. Otherwise, return them to the login page again
fn check_login(queries: &HashMap<String, String>, referer: &Option<&String>, domain_name: &str, db: &mut Client) -> Response {
    let user = queries.get("user");
    if user.is_none() { return BAD_REQUEST.clone_with_message("Query must have user field".to_string()); }
    let user = user.unwrap();
    let password = queries.get("pass");
    if password.is_none() { return BAD_REQUEST.clone_with_message("Query must have password field".to_string()); }
    let password = password.unwrap();
    
    // Get details from the database about the hash
    let row = db.query_opt("SELECT * FROM users WHERE username = $1", &[user]);
    if row.is_err() {
        return INTERNAL_SERVER_ERROR.clone_with_message(format!("Could not get username from db. Error: {}", row.unwrap_err().to_string()));
    }
    let row = row.unwrap();
    
    if row.is_none() {
        // no username with that 
        return OK.clone_with_message("User not found".to_string());
    }
    let row = row.unwrap();
    let hash: String = row.get(1); // 106 characters long
    let reset: bool = row.get(2);

    // A password hash is the username combined with the password, with an added salt
    let combined_password = format!("{}{}", user, password);
    if !sha512_crypt::verify(password, &hash) {
        return OK.clone_with_message("Password incorrect".to_string());
    }

    debug_println!("password matches");
    // PASSWORD MATCHES !
    if reset {
        let mut response = FOUND;
        response.add_header("Location", "/change-password".to_string());
        return response;
        todo!("Need to add refer_to")
    }
    debug_println!("No reset");
    
    // Password is okay
    let mut response = FOUND;
    add_cookie(&mut response, db, true, &user);
    debug_println!("Cookie added");
    // Check for return address in Referer link to decide which location to redirect to
    if let Some(referer) = referer {
        if let Ok((_, queries)) = http::parse_location(referer) {
            if let Some (location) = queries.get("return_to") {
                let location = urlencoding::decode(&location);
                if let Err(error) = location {
                    response.add_header("Location", "/".to_string());
                    return response;
                }
                let encoded_location = location.unwrap().into_owned();
                let location = urlencoding::decode(&encoded_location);
                if let Err(encoded_location) = location {
                    response.add_header("Location", "/".to_string());
                    return response;
                }
                debug_println!("location: {:?}", location);
                response.add_header("Location", location.unwrap().to_string());
                return response;
            }
        }
    }
    response.add_header("Location", "/".to_string());
    return response;
}

// Create a 
fn add_cookie(response: &mut Response, db: &mut Client, authenticated: bool, username: &str) -> Result<(), postgres::Error> {
    const COOKIE_LEN: usize = 225; // This size is arbitrary
    // This is not cryptographically secure
    let mut cookie = [0 as u8; COOKIE_LEN];
    for i in 0..COOKIE_LEN {
        cookie[i] = rand::random();
    }
    let cookie = general_purpose::STANDARD.encode(cookie);
    let rows_modified = db.execute("INSERT INTO cookies (cookie, username, authenticated, created) VALUES ($1, $2, $3, now());",
            &[&cookie, &username, &authenticated]);
    if let Err(error) = rows_modified {
        debug_println!("ERROR: {}", error);
        return Err(error);
    }
    response.add_header("Set-Cookie", format!("id={}; Path=/belgrade/", cookie));
    Ok(())
}

// Returns the stylesheet
fn get_styles(location: &str, content_root_dir: &str) -> Response {
    let stylesheet = read_all_bytes(&format!("{}{}", content_root_dir, location));
    let stylesheet = unwrap_either!(stylesheet);  
    // if stylesheet.is_err() {
    //     return stylesheet.unwrap_err();
    // } 
    // let stylesheet = stylesheet.unwrap();
    
    let mut response = OK;
    response.add_header("content-type", CSS.to_string());
    response.add_message(stylesheet);
    return response;
}

fn get_pdf(location: &str, content_root_dir: &str) -> Response {
    let mut response = OK;
    response.add_header("content-type", PDF.to_string());
    let pdf = read_all_bytes(&format!("{}{}", content_root_dir, location));
    let pdf = unwrap_either!(pdf);
    response.add_message(pdf);
    return response;
}

// Returns whether a document exists in the database
fn document_exists(queries: &HashMap<String, String>, db: &mut Client) -> Response {
    // Verify the appropriate queries exist
    if !queries.contains_key("crc32") { return BAD_REQUEST.clone_with_message("This request requires a crc32 query".to_string()); }
    let crc32_checksum = queries.get("crc32").unwrap();
    let crc32_checksum = crc32_checksum.parse::<u32>();
    if let Err(error) = crc32_checksum { return INTERNAL_SERVER_ERROR.clone_with_message(format!("Could not parse checksum into u32 from database. Error: {}", error.to_string())); }
    let crc32_checksum: u32 = crc32_checksum.unwrap();
    debug_println!("checksum: {} ... i32: {}", crc32_checksum, crc32_checksum as i32);

    if !queries.contains_key("type") { return BAD_REQUEST.clone_with_message("This request requires a type query".to_string()); }
    let pdf_type = queries.get("type").unwrap().parse::<i32>();
    if let Err(error) = pdf_type { return INTERNAL_SERVER_ERROR.clone_with_message(format!("Could not parse pdf_type into i32 from database. Error: {}", error.to_string())); }
    let pdf_type: i32 = pdf_type.unwrap();
    
    if !queries.contains_key("num") { return BAD_REQUEST.clone_with_message("This request requires a num query".to_string()); }
    let pdf_num = queries.get("num").unwrap().parse::<i32>();
    if let Err(error) = pdf_num { return INTERNAL_SERVER_ERROR.clone_with_message(format!("Could not parse pdf_num into i32 from database. Error: {}", error.to_string())); }
    let pdf_num: i32 = pdf_num.unwrap();

    debug_println!("Checksum: {}, Type: {}, Num: {}", crc32_checksum, pdf_type, pdf_num);
    let row = db.query_one("SELECT COUNT(*) FROM pdfs WHERE pdf_type = $1 AND pdf_num = $2 AND crc32_checksum = $3;",
            &[&pdf_type, &pdf_num, &(crc32_checksum as i32)]);
    if let Err(error) = row { return INTERNAL_SERVER_ERROR.clone_with_message(format!("Could not execute the document exists query on the database. Error: {}", error.to_string())); }
    let row = row.unwrap();
    let document_count: i64 = row.get(0);
    return OK.clone_with_message(document_count.to_string());
}

// Handles an http query to the database
fn handle_query(request: &HttpRequest, db: &mut Client, queries: &HashMap<String, String>, content_root_dir: &str, domain_name: &str) -> Response {
    if let Err(response) =  check_authentication(&request.location, queries, request.headers.get("Cookie"), domain_name, db) {
        return response;
    }
    // Ensure all fields are present and decoded. Set defaults for empty strings
    let query = request.query.get("query");
    if query.is_none() {return BAD_REQUEST.clone_with_message("Query must have 'query' field".to_string());}
    let query = query.unwrap();
    let query = urlencoding::decode(query);
    if let Err(error) = query { return BAD_REQUEST.clone_with_message(format!("Could not decode query into UTF-8: {}", error.to_string())); }
    let query = query.unwrap().into_owned();
    if query.contains("\"") || query.contains("'") { return BAD_REQUEST.clone_with_message("queries cannot have the \" or ' character in them.".to_string()); }

    let filter = request.query.get("filter");
    if filter.is_none() {return BAD_REQUEST.clone_with_message("Query must have 'filter' field".to_string());}
    let filter = filter.unwrap();
    let filter = urlencoding::decode(filter);
    if let Err(error) = filter { return BAD_REQUEST.clone_with_message(format!("Could not decode filter into UTF-8: {}", error.to_string())); }
    let filter = filter.unwrap().into_owned();
    if filter.contains("\"") || filter.contains("'") { return BAD_REQUEST.clone_with_message("filters cannot have the \" or ' characters in them.".to_string()); }

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
    const BASE_REQUEST: &str = r#"WITH r AS (SELECT CASE WHEN (e.customer = '') IS NOT FALSE THEN c.pdf_datetime ELSE e.pdf_datetime END, pdf_num, CASE WHEN (e.customer <> '') IS NOT FALSE THEN c.customer ELSE e.customer END, relative_path, "dt_path" FROM ( SELECT pdf_datetime, pdf_num, customer, relative_path FROM pdfs WHERE pdf_type = 1 ) AS e FULL JOIN ( SELECT pdf_num, pdf_datetime, customer, relative_path AS "dt_path" FROM pdfs WHERE pdf_type = 2 ) AS c USING (pdf_num)) SELECT * FROM r WHERE pdf_datetime BETWEEN $1 AND $2"#;
    // const BASE_REQUEST: &str = "SELECT pdf_datetime, pdf_type, pdf_num, customer, relative_path FROM pdfs WHERE pdf_datetime BETWEEN $1 AND $2";
    let full_query = match filter.as_str() {
        "Customer" => {
            format!("{} AND customer ILIKE '%{}%' ORDER BY pdf_num;", BASE_REQUEST, query)
        }
        "Number" => {
            let num = if let Ok(number) = query.parse::<u32>() { number } else { return BAD_REQUEST.clone_with_message("A valid number was not included in the search".to_string())};
            format!("{} AND pdf_num = {};", BASE_REQUEST, num )
        },
        _ => { 
            format!("{} AND relative_path ILIKE '%{}%' ORDER BY pdf_num;", BASE_REQUEST, query)
        }
    };
    
    // Execute query
    debug_println!("Query to be executed: {}", full_query);
    let rows = db.query(&full_query, &[&from_datetime, &to_datetime]);
    if let Err(error) = rows { return INTERNAL_SERVER_ERROR.clone_with_message(format!("Could not execute query on database: {}", error.to_string())); }
    let rows = rows.unwrap();
    
    // Create HTML table in response
    let entries = rows.len();
    let mut table = format!("<p>Found {} entries</p><table><tr><th>DateTime</th><th>Num</th><th>Customer</th><th>Batch Weights</th><th>Delivery Ticket</th></tr>", entries);
    for row in rows {
        let datetime: DateTime<Utc> = row.get(0);
        let pdf_num: i32 = row.get(1);
        let customer: &str = row.get(2);
        let bw_path: String = if let Ok(path) = row.try_get::<_, &str>(3) { format!("<a href=\"{}/belgrade/documents/{}\">Weights</a>", domain_name, path) } else { String::new() };
        let dt_path: String = if let Ok(path) = row.try_get::<_, &str>(4) { format!("<a href=\"{}/belgrade/documents/{}\">Ticket</a>", domain_name, path) } else { String::new() };
        // debug_println!("Datetime: {:?}, PDF Type: {}, Num: {}, Customer: {} Path: {}", datetime, pdf_type, pdf_num, customer, relative_path);

        let table_row = format!("<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                datetime.with_timezone(&GMTPlus4).format("%Y-%b-%d %I:%M %p").to_string(), pdf_num, customer, bw_path, dt_path);
        table.push_str(&table_row);
    }
    table.push_str("</table>");

    // Read from source html file and return appended file to client
    let mut index: Vec<u8> = vec![0;2048];
    let mut file = File::open(format!("{}/belgrade/documents/index.html", content_root_dir));
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

    let mut response = OK;
    response.add_message(index);
    response.add_header("content-type", HTML.to_string());
    return response;
}

fn is_authenticated(cookie: &str, db: &mut Client) -> bool {
    let row_result = db.query_opt("SELECT * FROM cookies where cookie = $1", &[&cookie]);
    if let Ok(optional_row) = row_result {
        if let Some(row) = optional_row {
            return true;
        }
    }
    return false;
}

fn authenticate(location: &str, queries: &HashMap<String, String>, domain_name: &str) -> Response {
    let mut response = FOUND;
    let mut return_to = urlencoding::encode(location).to_string();
    if queries.len() > 0 {
        return_to.push('?');
        for (key, value) in queries {
            return_to.push_str(&format!("{}={}&", key, value));
        }
    }
    let return_to = if queries.len() > 0 {
        urlencoding::encode(&return_to[..return_to.len()-1])
    } else {
        urlencoding::encode(&return_to)
    };
    response.add_header("Location", format!("/login?return_to={}", return_to));
    response.add_header("content-type", HTML.to_string());
    return response;
}

fn extract_cookie(cookie: &str) -> Option<&str> {
    // This is just hard coded right now.
    if cookie.len() > 3 {
        return Some(&cookie[3..]);
    } else {
        return None;
    }
}

fn check_authentication(location: &str, queries: &HashMap<String, String> ,cookie: Option<&String>, domain_name: &str, db: &mut Client) -> Result<(), Response> {
    if let Some(cookie) = cookie {
        if let Some(cookie) = extract_cookie(cookie) {
            if !is_authenticated(cookie, db) {
                return Err(authenticate(location, queries, domain_name));
            } else { // This is the only safe path 
                return Ok(())
            }
        } else {
            return Err(authenticate(location, queries, domain_name));
        }
    } else {
        return Err(authenticate(location, queries, domain_name));
    }
}

// Handles a get to belgrade
fn get_document_search(location: &str, queries: &HashMap<String, String>, cookie: Option<&String>, domain_name: &str, content_root_dir: &str, db: &mut Client) -> Response {
    if let Err(response) =  check_authentication(location, queries, cookie, domain_name, db) {
        return response;
    }

    let path = format!("{}{}/index.html", content_root_dir, location);
    debug_println!("{}", path);
    let page = read_all_bytes(&path);  
    if page.is_err() {
        return page.unwrap_err();
    } 
    let page = page.unwrap();
    
    let mut response = OK;
    response.add_header("content-type", HTML.to_string());
    response.add_message(page);
    return response;
}

// Handles a POST Http request. This is typically where PDFs are received,
// analyzed, and sorted into the correct location. Returns a success or error
// message.
fn handle_post(request: &HttpRequest, db: &mut Client, content_root_dir: &str) -> Response {
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

    // Deflate and retrieve date and customer.
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
    // FIXME: Improper handing of errors here can crash the program
    let mut datetime = Utc.timestamp_nanos(0);
    if pdf_type == PDFType::BatchWeight {
        let combined = format!("{} {}", &date, &time);
        debug_println!("date: {}, time: {}, combined: {}", &date, &time, &combined);
        let dt = GMTPlus4.datetime_from_str(&combined, "%e-%b-%Y %I:%M:%S %p");
        if let Ok(dt) = dt {
            datetime = dt.with_timezone(&Utc);
        } else {
            datetime = Utc.with_ymd_and_hms(1970, 1, 1, 12, 0, 0).unwrap();
        }
    } else if pdf_type == PDFType::DeliveryTicket {
        let combined = format!("{} 12:00:00", &date);
        debug_println!("date: {}", &combined);
        let dt = GMTPlus4.datetime_from_str(&combined, "%d/%m/%Y %H:%M:%S");
        if let Ok(dt) = dt {
            datetime = dt.with_timezone(&Utc);
        } else {
            datetime = Utc.with_ymd_and_hms(1970, 1, 1, 12, 0, 0).unwrap();
        } 
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
    let crc32 = Crc::<u32>::new(&crc::CRC_32_ISO_HDLC);
    let checksum = crc32.checksum(&pdf_as_bytes);

    let pdf_metadata = PDFMetadata { 
        datetime:       datetime, // NOTE: As of Dec 21 2022, this date uses a different format in Batch Weights vs Delivery Tickets
        pdf_type:       pdf_type,
        customer :      customer,
        relative_path:  relative_filepath,
        doc_number:     doc_number,
        crc32_checksum: checksum,
    };
    
    // Check whether pdf with this metadata already exists in database
    let row = db.query_one("SELECT COUNT(*) FROM pdfs WHERE crc32_checksum = $1 AND pdf_num = $2 AND pdf_type = $3;",
            &[&(pdf_metadata.crc32_checksum as i32), &pdf_metadata.doc_number, &(pdf_metadata.pdf_type as i32)]);
    if let Err(error) = row {return INTERNAL_SERVER_ERROR.clone_with_message(format!("Was not able to check if file already existed in server. Error: {}", error.to_string())) ;}
    let row = row.unwrap();
    if row.len() < 1 {return INTERNAL_SERVER_ERROR.clone_with_message("Tried to check if file already existed in database before adding it. Result from SQL query had no response when one was expected.".to_string()); }
    let count: i64 = row.get(0);
    debug_println!("Count: {}", count);
    if count > 0 {
        return OK.clone_with_message("File already exists in server. Taking no action.".to_string());
    } 

    // Place the PDF file into the correct place into the filesystem
    {
        let path_string = format!("{}{}{}", content_root_dir, "/belgrade/documents/", &pdf_metadata.relative_path);
        let path = Path::new(&path_string);
        let prefix = path.parent().unwrap(); // path without final component
        debug_println!("Prefix: {:?}", prefix);
        fs::create_dir_all(prefix).unwrap();
        let mut pdf_file = File::create(&path_string).unwrap();
        pdf_file.write_all(pdf_as_bytes.as_slice()).unwrap();
    }
    
    debug_println!("METADATA: {:#?}", pdf_metadata);

    // Store PDF into Database
    db.query(concat!("INSERT INTO pdfs (pdf_type, pdf_num, pdf_datetime, customer, relative_path, crc32_checksum)",
            "VALUES ($1, $2, $3, $4, $5, $6);"),
            &[&(pdf_metadata.pdf_type as i32),
            &pdf_metadata.doc_number,
            &datetime, 
            &pdf_metadata.customer,
            &pdf_metadata.relative_path,
            &(pdf_metadata.crc32_checksum as i32)]
    );


    // This is where the PDF should be parsed
    return CREATED.clone_with_message("PDF received and stored on server succesfully".to_owned());
}