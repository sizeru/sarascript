use std::{env, path, process, fs, error, io, default, f32::consts::PI};
use log::{trace, debug, info, warn, error};
use simplelog;
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

impl error::Error for Error {}

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
    let log = fs::File::options()
        .append(true)
        .create(true)
        .open(LOG_FILE)
        .expect("Could not create log file");
    simplelog::WriteLogger::init(
        simplelog::LevelFilter::Info, 
        simplelog::Config::default(),
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
            "backup" => server_backup(&cmdline_args),
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

#[derive(Clone, Copy, Serialize, Deserialize)]
enum DataCategories {
    All,
    TicketMetadata,
    TicketPdf,
    WeightMetadata,
    WeightPdf,
    Logs,
}

#[derive(Serialize, Deserialize)]
struct Config {
    backup_options: Option<BackupOptions>,
}

impl default::Default for Config {
    fn default() -> Self {
        Config {
            backup_options: Some(BackupOptions::default()),
        }
    }
}

impl Config {
    const USER_PATH: &str = ".config/rmc";
    const SHARED_PATH: &str = "/etc/xdg/rmc";
    const FILENAME: &str = "rmc.conf";

    fn user() -> Result<Config, Box<dyn error::Error>> {
        // TODO: Should create this folder if it does not exist
        let home = env::var("HOME")?;
        let result = fs::create_dir_all(Config::USER_PATH);
        let user_config_file = format!("{}/{}/{}", home, Config::USER_PATH, Config::FILENAME);
        let user_config = fs::File::open(user_config_file)?;
        let user_config_reader = io::BufReader::new(user_config);
        let config: Config = serde_json::from_reader(user_config_reader)?; 
        return Ok(config);
    }

    fn shared() -> Result<Config, Box<dyn error::Error>> {
        // TODO: Should create this folder if it does not exist
        let result = fs::create_dir_all(Config::SHARED_PATH);
        let shared_config_file = format!("{}/{}", Config::SHARED_PATH, Config::FILENAME);
        let shared_config = fs::File::open(shared_config_file)?;
        let shared_config_reader = io::BufReader::new(shared_config);
        let config: Config = serde_json::from_reader(shared_config_reader)?; 
        return Ok(config);
    }
}

#[derive(Serialize, Deserialize)]
struct BackupOptions {
    output_file: Option<path::PathBuf>,
    data: Option<DataCategories>,
    from_date: Option<String>, // TODO: Date should be a struct
    to_date: Option<String>,
}

impl default::Default for BackupOptions {
    fn default() -> Self {
        return BackupOptions {
            output_file: Some(path::PathBuf::from("rmc-server-backup.bkp")),
            data: Some(DataCategories::All),
            from_date: Some(String::new()),
            to_date: Some(String::new()),
        }    
    }
}

impl BackupOptions {
    pub fn parse() -> BackupOptions {
        let mut output_file = env::current_dir()
            .expect("Could not read the current directory");
        output_file.push("backup.tar.gz");
        let data = DataCategories::All;
        let from_date = String::new();
        let to_date = String::new();

        return BackupOptions{
            output_file: Some(output_file), 
            data: Some(data), 
            from_date: Some(from_date), 
            to_date: Some(to_date)
        };
    }


    pub fn retrieve(args: Option<&Vec<String>>) -> BackupOptions {
        let mut backup_options = BackupOptions::default(); 
        let shared_config = Config::shared();
        match shared_config {
            Ok(config) => backup_options.update(&config),
            Err(error) => warn!("Could not read from the shared config: {}", error.to_string()),
        }

        let user_config = Config::user();
        match user_config {
            Ok(config) => backup_options.update(&config),
            Err(error) => warn!("Could not read from the shared config: {}", error.to_string()),
        }

        return backup_options;
    }

    //
    fn update(&mut self, new_config: &Config) {
        if let Some(backup_options) = &new_config.backup_options {
            if let Some (data) = &backup_options.data {
                self.data = Some(data.clone());
            }
            if let Some(from_date) = &backup_options.from_date {
                self.from_date = Some(from_date.clone());
            }
            if let Some(to_date) = &backup_options.to_date {
                self.to_date = Some(to_date.clone());
            }
            if let Some(output_file) = &backup_options.output_file {
                self.output_file = Some(output_file.clone());
            }
        }
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

impl default::Default for ServerStatus {
    fn default() -> Self {
        return ServerStatus {
            state: ServerState::Stopped
        };
    }
}

impl ServerStatus {
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
        return Ok(ServerStatus::default()); // TODO: This should be the parsed data, not default
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
fn server_backup(args: &Vec<String>) -> Result<(), Box<dyn error::Error>> {
    info!("Creating a backup");
    let mut backup_options = BackupOptions::retrieve(Some(args));
   
    // Perform the backup
    Ok(())
}

fn update_backup_options(options: &mut BackupOptions, path: &path::PathBuf) {
    // TODO: Implement config parsing in order to set backup
}