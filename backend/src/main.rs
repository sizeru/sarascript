use std::{env, path::PathBuf, process};

fn main() {
    // Dispatch the correct function based off of the command
    let cmdline_args: Vec<String> = env::args().collect();

    if cmdline_args.len() == 1 {
        help();
        process::exit(0);
    }
    let function = cmdline_args[1].as_str();
    match function {
        "start" => server_start(),
        "stop" => server_stop(),
        "status" => server_status(),
        "logs" => server_logs(),
        "backup" => server_backup(),
        _ => help(),
    }
}

fn help() {
    dbg!("Command not recognized display help page");
}

#[repr(u8)]
enum DataCategories {
    All,
    TicketMetadata,
    TicketPdf,
    WeightMetadata,
    WeightPdf,
    Logs,
}

struct BackupOptions {
    output_file: PathBuf,
    data: DataCategories,
    from_date: String, // TODO: Date should be a struct
    to_date: String, 
}

impl BackupOptions {
    pub fn new() -> BackupOptions {
        let mut output_file = env::current_dir()
            .expect("Could not read the current directory");
        output_file.push("backup.tar.gz");
        let data = DataCategories::All;
        let from_date = String::new();
        let to_date = String::new();

        return BackupOptions{output_file, data, from_date, to_date};
    }
}

fn server_start() {
    dbg!("Starting server");
}
fn server_stop() {
    dbg!("Stopping server");
}
fn server_status() {
    dbg!("Checking server status");
}
fn server_logs() {
    dbg!("Printing server logs");
}
fn server_backup() {
    dbg!("Backing up server");
    let backup_options = config_get_backup_options();
    // Perform the backup
}


// Set options based on the config
fn config_get_backup_options() -> BackupOptions {
    // Set the default options, override them with options provided in the
    let mut backup_options = BackupOptions::new();
    let home = env::var("HOME").expect("Could not find $HOME environment variable");
    // TODO: Have these use paths based on XDG environment variables instead of
    // absolute paths
    let shared_config_file = PathBuf::from("/usr/share/rmc/rmc.conf");
    let user_config_file = PathBuf::from(format!("{}/.config/rmc/rmc.conf", home));
    
    update_backup_options(&mut backup_options, &shared_config_file);
    update_backup_options(&mut backup_options, &user_config_file);
    // TODO: Implement actual backing up of data using sanitized sql queries

    return backup_options;
}

fn update_backup_options(options: &mut BackupOptions, path: &PathBuf) {
    // TODO: Implement config parsing in order to set backup
}