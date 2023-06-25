// This rust module is meant to read and parse a config file It will always be
// faster to simply just read the file in order rather than to put it into a
// separate data structure first, but since this only runs at startup and allows
// the code to be much more easily extensible, I am deciding that it will be
// like this

// There is little error checking done on this code, since it is front facing
use std::{
    fs::File, 
    collections::HashMap, 
    io::{Error, Read, BufRead}, 
};

// Parses a config file and places the results in a hashmap
// This is meant to deal with small files, so it uses basic algorithms
pub fn parse<'a>(filepath: &str) -> Result<HashMap<String, String>, Error> {
    const BUFFER_SIZE: usize = 2048;

    let mut config = File::open(filepath)?;
    let mut buffer = [0; BUFFER_SIZE];
    let size = config.read(&mut buffer)?;

    let mut hashmap : HashMap<String, String> = HashMap::new();
    
    for line in buffer[0..size].lines() {
        if let Ok(line) = line {
            if line.starts_with(";") {
                continue;
            }

            if let Some ((key, value)) = line.split_once("=") {
                hashmap.insert(key.to_string(), value.to_string());
            }
            // hashmap.insert(pair[0], pair[1]);
        }
    }

    return Ok(hashmap)
}