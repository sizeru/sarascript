use std::{env, process, fs, error, io::{self, BufReader}, default, os::unix::net::{UnixDatagram, UnixStream}, string};
use log::{trace, debug, info, warn, error};
use simplelog;
use chrono;
use serde::{Serialize, Deserialize, Serializer};
use std::os::unix::fs::DirBuilderExt;
use server::{ServerState, ServerStatus, Server, Error};

use crate::server::{IPCCommand, IPCResponse};
mod server;
mod http;
mod config_parser;

const LOG_FILE: &str = "/var/log/rmc/log";

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
            "start" => server_start().await,
            "run" => server::run().await,
            "stop" => server_stop().await,
            "status" => server_status().await,
            "backup" => server_backup(&cmdline_args).await,
            "user" => server_user(&cmdline_args).await,
            "create" => server_create().await,
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
    local_server_user: Option<String>,
    backup_options: Option<BackupOptions>,
}

impl default::Default for Config {
    fn default() -> Self {
        Config {
            backup_options: Some(BackupOptions::default()),
            local_server_user: Some("rmc-local".to_string()),
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
        let user_config_file = format!("{}/{}/{}", home, Config::USER_PATH, Config::FILENAME);
        let user_config = fs::File::open(user_config_file)?;
        let user_config_reader = io::BufReader::new(user_config);
        let config: Config = serde_json::from_reader(user_config_reader)?; 
        return Ok(config);
    }

    fn shared() -> Result<Config, Box<dyn error::Error>> {
        // TODO: Should create this folder if it does not exist
        let shared_config_file = format!("{}/{}", Config::SHARED_PATH, Config::FILENAME);
        let shared_config = fs::File::open(shared_config_file)?;
        let shared_config_reader = io::BufReader::new(shared_config);
        let config: Config = serde_json::from_reader(shared_config_reader)?; 
        return Ok(config);
    }
}

#[derive(Serialize, Deserialize)]
struct BackupOptions {
    output_file: Option<String>,
    data: Option<DataCategories>,
    from_date: Option<String>, // TODO: Date should be a struct
    to_date: Option<String>,
}

impl default::Default for BackupOptions {
    fn default() -> Self {
        return BackupOptions {
            output_file: Some(format!("rmc_server_backup-{}.bkp", chrono::offset::Local::now().format("%Y_%b_%d"))),
            data: Some(DataCategories::All),
            from_date: Some(String::new()),
            to_date: Some(String::new()),
        }    
    }
}

impl BackupOptions {
    /// Retrieve and set options for this backup based on Config options and command line
    /// options. Program will not fail if a config cannot be accessed. Program
    /// will fail if the cmdline is malformed in any way.
    pub fn retrieve(cmdline_args: Option<&Vec<String>>) -> Result<BackupOptions, Box<dyn error::Error>> {
        let mut backup_options = BackupOptions::default(); 
        let shared_config = Config::shared();
        match shared_config {
            Ok(config) => backup_options.update(&config.backup_options),
            Err(error) => warn!("Could not read from the shared config. Error: {}", error.to_string()),
        }

        let user_config = Config::user();
        match user_config {
            Ok(config) => backup_options.update(&config.backup_options),
            Err(error) => warn!("Could not read from the user config. Error: {}", error.to_string()),
        }

        if let Some(args) = cmdline_args {
            for i in 2..args.len() {
                let arg = &args[i]; 
                let split: Vec<&str> = arg.split('=').collect();
                if split.len() != 2 {
                    return Err(Box::new(Error::MalformedCmdline(args[i].clone())));
                }
                let key = *split.get(0).unwrap();
                let value = *split.get(1).unwrap();
                match key {
                    "--output-file" => {
                        backup_options.output_file = Some(value.to_owned()); 
                    },
                    "--data" => todo!(),
                    "--from-date" => todo!(),
                    "--to-date" => todo!(),
                    _ => return Err(Box::new(Error::MalformedCmdline(args[i].clone()))),
                }
            }
        }
        return Ok(backup_options);
    }

    /// Update the backup options with any options from a new BackupOptions struct
    fn update(&mut self, backup_options: &Option<BackupOptions>) {
        if let Some(backup_options) = backup_options {
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


// Create a new daemon process which will run the server.
async fn server_start() -> Result<(), Box<dyn error::Error>> {
    // Check if server is already started
    let status = get_server_status().await?;
    if status.state != ServerState::Stopped && status.state != ServerState::Unreachable {
        return Err(Box::new(Error::ServerAlreadyStarted(status)));
    }
    let daemon  = process::Command::new("rmc")
        .arg("run")
        .spawn()?;
    info!("Preparing to daemonize server");
    // Open pipe from child to read the PID 
    // TODO: wait for signal from child that server has been started succesfully.
    return Ok(());
}

/// Checks on the status of the server. If the server status file cannot be found, 
async fn get_server_status() -> Result<ServerStatus, Box<dyn error::Error>> {
    let response = Server::exec_ipc_message(&IPCCommand::GetStatus).await?;
    println!("Got response");
    match response {
        IPCResponse::Status(server_status) => {
            return Ok(server_status);
        },
        IPCResponse::CannotConnect => {
            return Ok(ServerStatus::UNREACHABLE);
        },
        _ => { 
            return Err(Box::new(Error::InvalidIPCReponse(IPCResponse::Status(ServerStatus::default()), response))); 
        }
    }
    // todo!("Check for the socket file. If not present. Check for the heartbeat file.
    // Socket => query the status over the socket. If this fails go to another branch
    // !Socket + Heartbeat => Server is running but inaccessible
    // !Socket + !Heartbeat => Server is likely offline
    // ");
    // return Err(Box::new(Error::MissingServerFile));
}

/// Safely stop the server after the last process is complete.
async fn server_stop() -> Result<(), Box<dyn error::Error>> {
    info!("Sending signal to stop server");
    let pid_file = Server::pid_file()?;
    let contents = fs::read_to_string(pid_file)?;
    println!("pid: {}", contents);
    let pid = contents.parse::<usize>()?;
    
    let result = process::Command::new("kill")
        .arg("-s")
        .arg("SIGINT")
        .arg(pid.to_string())
        .output()?;
    Ok(())
}

/// Get status information about the server daemon
async fn server_status() -> Result<(), Box<dyn error::Error>> {
    warn!("Checking server status");
    let status = get_server_status().await?;
    println!("{:?}", status);
    Ok(())
}

/// Creates a backup of the server with some optional arguments
async fn server_backup(args: &Vec<String>) -> Result<(), Box<dyn error::Error>> {
    let backup_options = BackupOptions::retrieve(Some(args))?;
    info!("Creating a backup at: {}", backup_options.output_file.unwrap());
    todo!("Implement");
    // Perform the backup
    Ok(())
}

/// Create files needed for a new server
async fn server_create() -> Result<(), Box<dyn error::Error>> {
    info!("Creating the directories needed for a new server");
    let runtime_dir = Server::runtime_dir().unwrap();
    fs::DirBuilder::new()
        .mode(0o700)
        .recursive(true)
        .create(runtime_dir)?;
    fs::DirBuilder::new()
        .mode(0o700)
        .recursive(true)
        .create(Server::SYSTEM_CONFIG_DIR)?;
    let user_config_dir = Server::user_config_dir().unwrap();
    fs::DirBuilder::new()
        .mode(0x700)
        .recursive(true)
        .create(&user_config_dir)?;
    Ok(())
}
    
/// Allows editting of user settings, such as generating new users and
/// passwords, listing of users, editing permissions, etc.
async fn server_user(args: &Vec<String>) -> Result<(), Box<dyn error::Error>> {
    todo!("Need to implement this. Generating new users & passwords is top priority")
}
