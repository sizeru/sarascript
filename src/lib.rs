use std::{io::Error, fs, fs::{File}, path::{PathBuf, Path}};
use regex::bytes::Regex;


const COMMENT_PREFIX: &[u8; 4]  = b"<!--";
const COMMENT_POSTFIX: &[u8; 3] = b"-->";

// LOGIC:
//     process
//         convert all files in cwd into new dir prepped-html
//     process <dir>
//         same but use files from <dir>
//     process <dir> <dir2>
//         same but put files into dir2

/// Process a file and return it as 
pub fn process(path: &Path, output_dir: &Path) -> Result<(), Error> 
{
    if output_dir.exists() {
        todo!("Don't allow overwriting");
    }
    if path.is_dir() {
        process_dir(path, output_dir)?;
    } else if path.is_file() {
        process_file(path, output_dir)?;
    }
    return Ok(())
}

/// Recursively tries to process every file in a directory
fn process_dir(dir: &Path, output_dir: &Path) -> Result<(), Error> {
    let dir_entries = dir.read_dir()?;
    for entry in dir_entries {
        let file = entry?;
        let file_type = file.file_type()?;
        let new_file = output_dir.join(file.file_name());
        if file_type.is_dir() {
            process_dir(&file.path(), &new_file)?; 
        } else if file_type.is_file() {
            process_file(&file.path(), &new_file)?;
        } else {
            panic!("Cannot parse simlinks");
        }
    }
    Ok(())
} 

// Regex version of file processing
pub fn process_file(file: &Path, output_file: &Path) -> Result<(), Error> {
    let re = Regex::new("<!--.*-->").unwrap();
    let now = std::time::Instant::now();
    let contents = std::fs::read(file)?;
    let captures = re.captures(&contents);
    if let Some(captures) = captures {
        for capture in captures.iter() {
            if let Some(capture) = capture {
                println!("match: {:?}", capture);
            }
        }
    }
    println!("{:?}", now.elapsed());

    // contents
    //     .windows(COMMENT_PREFIX.len())
    //     .enumerate()
    //     .filter(|&(_index, string)| string.eq(COMMENT_PREFIX))
    //     .for_each(|(comment_start_index, _)| {
    //         let end_index = contents[comment_start_index..]
    //             .windows(COMMENT_POSTFIX.len())
    //             .enumerate()
    //             .find(|&(comment_end_index, window)| window.eq(COMMENT_POSTFIX));
    //         if let Some((end_index, _)) = end_index {
    //             let new_start = comment_start_index + COMMENT_PREFIX.len();
    //             let new_end = comment_start_index + end_index;
    //             let comment = &contents[new_start..new_end];
    //             println!("Comment: {}", std::str::from_utf8(comment).unwrap());
    //         }
    //     }
    // );
    // println!("{:?}", now.elapsed());


    // let end_delimiter = '\"' as u8;
    // let mut lengths: Vec<usize> = Vec::new();
    // for &index in &include_indices {
    //     let end_index = buffer[index..]
    //         .iter()
    //         .position(|&char| char.eq(&end_delimiter));
    //     if let Some(end_index) = end_index {
    //         lengths.push(end_index);
    //     }
    // }
    // if include_indices.len() != lengths.len() {
    //     return Err(Response::builder()
    //         .status(StatusCode::INTERNAL_SERVER_ERROR)
    //         .body(Bytes::from("Somebody didn't format the HTML include comment correctly"))
    //         .unwrap());
    // }
    // let mut include_comments: Vec<HtmlIncludeComment> = Vec::new();
    // for i in 0..include_indices.len() {
    //     let slice = &buffer[include_indices[i]..include_indices[i]+lengths[i]]; 
    //     match std::str::from_utf8(slice) {
    //         Ok(filepath) => {
    //             // NOTE: The adding and subtracting done here is so that the
    //             // length and size include the HTML comment itself, rather than
    //             // just the filename
    //             let html_include_comment = HtmlIncludeComment {
    //                 start_index: include_indices[i] - include_prefix.len(),
    //                 length: lengths[i] + include_prefix.len() + 4,
    //                 include_file: filepath,
    //             };
    //             include_comments.push(html_include_comment);
    //         }
    //         Err(_) => {
    //             return Err(Response::builder()
    //                 .status(StatusCode::INTERNAL_SERVER_ERROR)
    //                 .body(Bytes::from("Invalid utf-8 in HTML include comment"))
    //                 .unwrap());
    //         }
    //     }
    // }
    // include_comments.sort_by(|a, b| a.start_index.cmp(&b.start_index).reverse());
    // return Ok(include_comments.into());

    Ok(())
}

// Returns all filenames which are to be included in this struct. The returned
// vec is guaranteed to be sorted from largest index to smallest index (so that
// when iterating through names, you will corrupt previous indices by inserting
// html
// fn get_html_includes(buffer: &[u8]) -> Result<Rc<[HtmlIncludeComment]>, Response<Bytes>> {
//     let include_prefix = "<!--#include \"".as_bytes().to_vec();

//     let include_indices: Vec<usize> = buffer
//         .windows(include_prefix.len())
//         .enumerate()
//         .filter(|&(_index, string)| string.eq(&include_prefix[..]))
//         .map(|(index, _string)| index + include_prefix.len())
//         .collect();

//     let end_delimiter = '\"' as u8;
//     let mut lengths: Vec<usize> = Vec::new();
//     for &index in &include_indices {
//         let end_index = buffer[index..]
//             .iter()
//             .position(|&char| char.eq(&end_delimiter));
//         if let Some(end_index) = end_index {
//             lengths.push(end_index);
//         }
//     }
//     if include_indices.len() != lengths.len() {
//         return Err(Response::builder()
//             .status(StatusCode::INTERNAL_SERVER_ERROR)
//             .body(Bytes::from("Somebody didn't format the HTML include comment correctly"))
//             .unwrap());
//     }
//     let mut include_comments: Vec<HtmlIncludeComment> = Vec::new();
//     for i in 0..include_indices.len() {
//         let slice = &buffer[include_indices[i]..include_indices[i]+lengths[i]]; 
//         match std::str::from_utf8(slice) {
//             Ok(filepath) => {
//                 // NOTE: The adding and subtracting done here is so that the
//                 // length and size include the HTML comment itself, rather than
//                 // just the filename
//                 let html_include_comment = HtmlIncludeComment {
//                     start_index: include_indices[i] - include_prefix.len(),
//                     length: lengths[i] + include_prefix.len() + 4,
//                     include_file: filepath,
//                 };
//                 include_comments.push(html_include_comment);
//             }
//             Err(_) => {
//                 return Err(Response::builder()
//                     .status(StatusCode::INTERNAL_SERVER_ERROR)
//                     .body(Bytes::from("Invalid utf-8 in HTML include comment"))
//                     .unwrap());
//             }
//         }
//     }
//     include_comments.sort_by(|a, b| a.start_index.cmp(&b.start_index).reverse());
//     return Ok(include_comments.into());
// }

// fn insert_html_includes(raw_html: &[u8], include_files: &[HtmlIncludeComment]) -> Result<Rc<[u8]>, Response<Bytes>> {
//     let mut html: Vec<u8> = Vec::with_capacity(raw_html.len()); 
//     html.resize(raw_html.len(), 0);
//     html.copy_from_slice(raw_html);
//     for include_file in include_files {
//         let mut external_file: File;
//         {
//             let config = CONFIG.read().unwrap();
//             let root = &config.web_content_root_dir;
//             external_file = file_open(&format!("{root}{}", include_file.include_file))?;
//         };
//         let mut external_content = Vec::new();
//         if let Err(_) = external_file.read_to_end(&mut external_content) {
//             return Err(Response::builder()
//                 .status(StatusCode::INTERNAL_SERVER_ERROR)
//                 .body(Bytes::from("Error when reading contents of external file."))
//                 .unwrap());
//         }
//         let range = include_file.start_index .. include_file.start_index+include_file.length;
//         html.splice(range, external_content);
//     }
//     return Ok(html.into())
// }

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn time() {
        let source = std::path::Path::new("tests/needle_in_haystack.html");
        process(source, Path::new("./out")).unwrap();
    }
}