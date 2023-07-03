use htmlprep::*;

const OUTPUT_DIR: &str = "out";
const WEBROOT: &str = ".";

#[test]
fn time() {
    compile("tests/needle_in_haystack.html", OUTPUT_DIR, WEBROOT).unwrap();
}

#[test]
fn bad_format() {
    compile("tests/bad_format1.html", OUTPUT_DIR, WEBROOT).unwrap()
}

#[test]
fn two_includes() {
    compile("tests/two_includes.html", OUTPUT_DIR, "/").unwrap();
}

#[test]
fn two_includes2() {
    compile("tests/two_includes2.html", OUTPUT_DIR, "/").unwrap();
}

#[test]
fn entire_test_dir() {
    compile("tests", "all_tests", WEBROOT).unwrap();
}