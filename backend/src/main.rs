use std::{env, path::PathBuf, process, fs::{File, self}, error};
use log::{trace, debug, info, warn, error};
use simplelog::{WriteLogger, LevelFilter, Config};
use serde::{Serialize, Deserialize};
const LOG_FILE: &str = "rmc.log";
const SERVER_FILE: &str = "server_status";
const EXECUTABLE_NAME: &str = "rmc";
#[derive(Debug)]
enum Error {
    ServerAlreadyStarted(ServerStatus),
    MissingServerFile,
    CannotFindExecutable,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ServerAlreadyStarted(current_status) => {
                write!(f, "cannot start a server that is already running. 
                    Server status at time of command: {}", current_status.to_string())
            },
            Self::MissingServerFile => {
                write!(f, "Could not find server file `{}` in working directory: {}", SERVER_FILE, env::current_dir().unwrap().display())
            },
            Self::CannotFindExecutable => {
                write!(f, "Could not find executable `{}` in working directory: {}", EXECUTABLE_NAME, env::current_dir().unwrap().display())
            },
        } 
    }

}

impl std::error::Error for Error {}

/// The intended method for a first time user is as follows
/// 
/// ```
///     rmc init
///     rmc start
/// ```
/// 
/// If something goes wrong at any point, the server will mention it. Most
/// problems can be detected using `rmc status` and repaired either manually
/// or by using `rmc repair` to force a repair

#[tokio::main]
async fn main() {
    let log = File::options()
        .append(true)
        .create(true)
        .open(LOG_FILE)
        .expect("Could not create log file");
    WriteLogger::init(
        LevelFilter::Info, 
        Config::default(),
        log
    ).expect("Could not initialize logger");
    
    // Dispatch the correct function based off of the command
    let cmdline_args: Vec<String> = env::args().collect();
    if let Some(function) = cmdline_args.get(1) {
        let result = match function.as_str() {
            "start" => server_start(),
            "run" => server_run(),
            "stop" => server_stop(),
            "status" => server_status(),
            "backup" => server_backup(),
            _ => help(),
        };
        if let Err(error) = result {
            println!("Error: {}", error.to_string());
        }
    } else {
        println!("Invalid usage. Type `rmc help` for usage information");
    }
    return;
}

/// Display the help page
fn help() -> Result<(), Box<dyn error::Error>> {
    debug!("Command not recognized. display help page");
    Ok(())
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

/// Runs the server inside of the current process. No daemon. 
fn server_run() -> Result<(), Box<dyn error::Error>> {
    debug!("Running the server in shell terminal. No daemon");
    Ok(())
}

// Create a new daemon process which will run the server.
fn server_start() -> Result<(), Box<dyn error::Error>> {
    // Check if server is already started
    let status = parse_server_status()?;
    if status.state != ServerState::Stopped {
        return Err(Box::new(Error::ServerAlreadyStarted(status)));
    }
    let daemon  = process::Command::new("target/debug/rmc")
        .arg("run")
        .spawn()?;
    info!("Starting server as a daemon");
    return Ok(());
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
enum ServerState {
    Stopped,
    Starting,
    Running,
    Terminating,
}

#[derive(Serialize, Deserialize, Debug)]
struct ServerStatus {
    state: ServerState,    
}

impl ServerStatus {
   
    /// The DEFAULT values for a server 
    const DEFAULT: ServerStatus = ServerStatus {
        state: ServerState::Stopped
    };
    
    pub fn store(&self) -> Result<(), Box<dyn error::Error>> {
        let json_string = serde_json::to_string(self)?;
        fs::write(SERVER_FILE, json_string)?;
        return Ok(());
    }

    pub fn print(&self) {
        println!("{}", self.to_string());
    }

    pub fn to_string(&self) -> String {
        return "Unimplemented!".to_string();
    }

    pub fn parse() -> Result<ServerStatus, Box<dyn error::Error>> {
        let status_file = fs::read_to_string(SERVER_FILE)?;
        for line in status_file.lines() {
            let values: Vec<&str> = line.split("=").collect();
            let x = values.get(0).unwrap();
            let y = values.get(1).unwrap();
        }
        return Ok(ServerStatus::DEFAULT); // TODO: This should be the parsed data, not default
    }
}

/// Checks on the status of the server. If the server status file cannot be found, 
fn parse_server_status() -> Result<ServerStatus, Box<dyn error::Error>> {
    return Err(Box::new(Error::MissingServerFile));
}

/// Safely stop the server after the last process is complete.
fn server_stop() -> Result<(), Box<dyn error::Error>> {
    info!("Stopping server");
    Ok(())
}

/// Get status information about the server daemon
fn server_status() -> Result<(), Box<dyn error::Error>> {
    warn!("Checking server status");
    Ok(())
}

/// Creates a backup of the server with some optional arguments
fn server_backup() -> Result<(), Box<dyn error::Error>> {
    dbg!("Backing up server");
    let backup_options = config_get_backup_options();
    // Perform the backup
    Ok(())
}


/// Set options based on the config
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