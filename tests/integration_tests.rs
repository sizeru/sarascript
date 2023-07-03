use htmlprep::process;

const OUTPUT_DIR: &str = "out";
const WEBROOT: &str = ".";

#[test]
fn time() {
    process("tests/needle_in_haystack.html", OUTPUT_DIR, WEBROOT).unwrap();
}

#[test]
fn bad_format() {
    process("tests/bad_format1.html", OUTPUT_DIR, WEBROOT).unwrap()
}

#[test]
fn two_includes() {
    process("tests/two_includes.html", OUTPUT_DIR, "/").unwrap();
}

#[test]
fn two_includes2() {
    process("tests/two_includes2.html", OUTPUT_DIR, "/").unwrap();
}

#[test]
fn entire_test_dir() {
    process("tests", "all_tests", WEBROOT).unwrap();
}