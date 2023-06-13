use std::{env, process, fs, error, io, sync::RwLock};
use std::os::unix::fs::DirBuilderExt;
use log::{info, warn}; // TODO: Add appropriate trace, debug, error logs
use simplelog;
use serde::{Serialize, Deserialize};
mod server;
use server::{ServerState, ServerStatus, Server, Error, IPCCommand, IPCResponse};

const LOG_FILE: &str = "/var/log/rmc/rmc.log";
// The type is Box<Config> rather than Config so that it is trivial to replace the Config when needed
static CONFIG: RwLock<Config> = RwLock::new(Config::EMPTY);

/// This comment is outdated. Right now there is an install script
/// and chances are the server should be run as a daemon using
/// systemctl most times:
///  
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
    // TODO: Make the debug logger appear in the println only in debug builds
    simplelog::WriteLogger::init(
        simplelog::LevelFilter::Info, 
        simplelog::Config::default(),
        log
    ).expect("Could not initialize logger");
    {
        let config = &CONFIG;
        let mut config_writer = config.write().unwrap();
        config_writer.init_default();
    }
    if let Err(error) = Config::load() {
        println!("{error}");
        process::exit(1);
    }
    
    // Dispatch the correct function based off of the command
    let cmdline_args: Vec<String> = env::args().collect();
    if let Some(function) = cmdline_args.get(1) {
        let result = match function.as_str() {
            "start" => server_start().await,
            "run" => server::run().await,
            "stop" => server_stop(&cmdline_args).await,
            "status" => server_status().await,
            // "backup" => server_backup(&cmdline_args).await,
            "user" => server_user(&cmdline_args).await,
            "create" => server_create().await,
            "help" => help(),
            _ => Err(Box::new(Error::UnknownUsage(cmdline_args.iter().map(|x| format!("{} ", x)).collect::<String>())).into()),
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
    println!("Help page summoned. TODO: Add a help page");
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

#[derive(Deserialize)]
struct ParsedConfig {
    // backup_options: Option<BackupOptions>,
    web_content_root_dir: Option<String>,
    ipv4_address: Option<String>,
    postgres_connection_uri: Option<String>,
}


struct Config {
    web_content_root_dir: String,
    ipv4_address: String,
    postgres_connection_uri: String,
}


impl Config {
    const EMPTY: Config = Config {
        web_content_root_dir: String::new(), //"/var/www/rmc".to_owned(),
        ipv4_address: String::new(), // "127.0.0.1:7878".to_owned(),
        postgres_connection_uri: String::new(), // "postgresql://localhost:5432/rmc".to_owned(),
    };

    fn init_default(&mut self) {
        self.web_content_root_dir = String::from("/var/www/rmc");
        self.ipv4_address = String::from("127.0.0.1:7878");
        self.postgres_connection_uri = String::from("postgresql://localhost:5432/rmc");
    }

    fn update(&mut self, new_config: &ParsedConfig) {
        if let Some(web_content_root_dir) = &new_config.web_content_root_dir {
            self.web_content_root_dir = web_content_root_dir.clone();
        }    
        if let Some(ipv4_address) = &new_config.ipv4_address {
            self.ipv4_address = ipv4_address.clone();
        }
        if let Some(postgres_connection_uri) = &new_config.postgres_connection_uri {
            self.postgres_connection_uri = postgres_connection_uri.clone();
        }
    }

    // Loads config settings from the filesystem into the CONFIG singleton.
    pub fn load() -> Result<(), Box<dyn error::Error>> {
        let shared_config = ParsedConfig::shared()?;
        let config = &CONFIG;
        let mut config_writer = config.write().unwrap();
        config_writer.update(&shared_config);
        return Ok(());
    }
}


impl ParsedConfig {
    const USER_PATH: &str = ".config/rmc";
    const SHARED_PATH: &str = "/etc/xdg/rmc";
    const FILENAME: &str = "rmc.conf";

    // User config is not currently used
    // fn user() -> Result<Config, Box<dyn error::Error>> {
    //     // TODO: Should create this folder if it does not exist
    //     let home = env::var("HOME")?;
    //     let user_config_file = format!("{}/{}/{}", home, Config::USER_PATH, Config::FILENAME);
    //     let user_config = fs::File::open(user_config_file)?;
    //     let user_config_reader = io::BufReader::new(user_config);
    //     let config: Config = serde_json::from_reader(user_config_reader)?; 
    //     return Ok(config);
    // }

    fn shared() -> Result<ParsedConfig, Box<dyn error::Error>> {
        // TODO: Should create this folder if it does not exist
        let shared_config_file = format!("{}/{}", ParsedConfig::SHARED_PATH, ParsedConfig::FILENAME);
        let shared_config = fs::File::open(shared_config_file)?;
        let shared_config_reader = io::BufReader::new(shared_config);
        let parsed_config: Result<ParsedConfig, serde_json::Error> = serde_json::from_reader(shared_config_reader); 
        match parsed_config {
            Ok(parsed_config) => {
                return Ok(parsed_config);
            },
            Err(error) => {
                return Err(Error::ConfigParseError(
                    format!("{}/{}", ParsedConfig::SHARED_PATH, ParsedConfig::FILENAME), Box::new(error)
                ).into());
            }
        }
    }
}

// #[derive(Serialize, Deserialize)]
// struct BackupOptions {
//     output_file: Option<String>,
//     data: Option<DataCategories>,
//     from_date: Option<String>, // TODO: Date should be a struct
//     to_date: Option<String>,
// }

// impl default::Default for BackupOptions {
//     fn default() -> Self {
//         return BackupOptions {
//             output_file: Some(format!("rmc_server_backup-{}.bkp", chrono::offset::Local::now().format("%Y_%b_%d"))),
//             data: Some(DataCategories::All),
//             from_date: Some(String::new()),
//             to_date: Some(String::new()),
//         }    
//     }
// }

// impl BackupOptions {
//     /// Retrieve and set options for this backup based on Config options and command line
//     /// options. Program will not fail if a config cannot be accessed. Program
//     /// will fail if the cmdline is malformed in any way.
//     pub fn retrieve(cmdline_args: Option<&Vec<String>>) -> Result<BackupOptions, Box<dyn error::Error>> {
//         let config = CONFIG.read().unwrap();
//         let backup_options = config.backup_options.unwrap();
//         if let Some(args) = cmdline_args {
//             for i in 2..args.len() {
//                 let arg = &args[i]; 
//                 let split: Vec<&str> = arg.split('=').collect();
//                 if split.len() != 2 {
//                     return Err(Box::new(Error::MalformedCmdline(args[i].clone())));
//                 }
//                 let key = *split.get(0).unwrap();
//                 let value = *split.get(1).unwrap();
//                 match key {
//                     "--output-file" => {
//                         backup_options.output_file = Some(value.to_owned()); 
//                     },
//                     "--data" => todo!(),
//                     "--from-date" => todo!(),
//                     "--to-date" => todo!(),
//                     _ => return Err(Box::new(Error::MalformedCmdline(args[i].clone()))),
//                 }
//             }
//         }
//         return Ok(backup_options);
//     }

//     /// Update the backup options with any options from a new BackupOptions struct
//     fn update(&mut self, backup_options: &Option<BackupOptions>) {
//         if let Some(backup_options) = backup_options {
//             if let Some (data) = &backup_options.data {
//                 self.data = Some(data.clone());
//             }
//             if let Some(from_date) = &backup_options.from_date {
//                 self.from_date = Some(from_date.clone());
//             }
//             if let Some(to_date) = &backup_options.to_date {
//                 self.to_date = Some(to_date.clone());
//             }
//             if let Some(output_file) = &backup_options.output_file {
//                 self.output_file = Some(output_file.clone());
//             }
//         }
//     }
// }


// Create a new daemon process which will run the server.
async fn server_start() -> Result<(), Box<dyn error::Error>> {
    // Check if server is already started
    let status = get_server_status().await?;
    if status.state != ServerState::Stopped && status.state != ServerState::Unreachable {
        return Err(Box::new(Error::ServerAlreadyStarted(status)));
    }
    process::Command::new("rmc")
        .arg("run")
        // .stdout(process::Stdio::piped())
        .spawn()?;
    println!("Daemon spawned");

    // TODO: It would be nice for the spawned daemon to signal to the parent
    // that it started succesfully. Although, since this is intended to be a
    // systemd daemon, the `run` command will likely be preferred to the
    // `start` command. This function is likely to be for testing only.

    // let mut child_output = io::BufReader::new(daemon.stdout.as_mut().take().unwrap());
    // let mut output_line = String::new();
    // println!("Prepared a reader");
    // let bytes_read = child_output.read_to_string(&mut output_line)?;
    // println!("Read {bytes_read} bytes");
    // if output_line.eq(Server::STARTING_MESSAGE) {
    //     return Ok(())
    // } else {
    //     return Err(Error::CouldNotStartServer(output_line).into());
    // }
    return Ok(());
}

/// Checks on the status of the server. If the server status file cannot be found, 
async fn get_server_status() -> Result<ServerStatus, Box<dyn error::Error>> {
    let response = Server::exec_ipc_message(&IPCCommand::GetStatus).await?;
    match response {
        IPCResponse::Status(server_status) => {
            return Ok(server_status);
        },
        IPCResponse::CannotConnect => {
            return Ok(ServerStatus::UNREACHABLE);
        },
        // This is unreachable
        // _ => { 
        //     return Err(Box::new(Error::InvalidIPCReponse(IPCResponse::Status(ServerStatus::default()), response))); 
        // }
    }
    // todo!("Check for the socket file. If not present. Check for the heartbeat file.
    // Socket => query the status over the socket. If this fails go to another branch
    // !Socket + Heartbeat => Server is running but inaccessible
    // !Socket + !Heartbeat => Server is likely offline
    // ");
    // return Err(Box::new(Error::MissingServerFile));
}

/// Safely stop the server after the last process is complete.
async fn server_stop(cmdline_args: &Vec<String>) -> Result<(), Box<dyn error::Error>> {
    info!("Sending signal to stop server");
    let pid_file = Server::pid_file()?;
    let contents = fs::read_to_string(pid_file)?;
    println!("pid: {}", contents);
    let pid = contents.parse::<usize>()?;

    if let Some(flag) = cmdline_args.get(2) {
        if flag.eq("--force") {
            let result = process::Command::new("kill")
                .arg("-s")
                .arg("SIGKILL")
                .arg(pid.to_string())
                .output();
            match result {
                Ok(output) => {
                    let runtime_dir = Server::runtime_dir().unwrap();
                    fs::remove_dir_all(runtime_dir).unwrap();
                    println!("Forced closed and manually cleaned up runtime files.\n{:?}", output);
                    return Ok(())
                }
                Err(error) => return Err(error.into()),
            }
        }
        return Err(Box::new(Error::UnknownUsage(cmdline_args.iter().map(|x| format!("{} ", x)).collect::<String>())))
    } 
    
    println!("Sending");
    process::Command::new("kill")
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
    println!("{}", status);
    Ok(())
}

/// Creates a backup of the server with some optional arguments
async fn server_backup(args: &Vec<String>) -> Result<(), Box<dyn error::Error>> {
    todo!("Implement");
    // let backup_options = BackupOptions::retrieve(Some(args))?;
    // info!("Creating a backup at: {}", backup_options.output_file.unwrap());
    // // Perform the backup
    // Ok(())
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
    
/// Allows editing of user settings, such as generating new users and
/// passwords, listing of users, editing permissions, etc.
async fn server_user(args: &Vec<String>) -> Result<(), Box<dyn error::Error>> {
    todo!("Need to implement this. Generating new users & passwords is top priority");
    // let status = get_server_status().await?;
    // if status.state != ServerState::Running {
    //     println!("Cannot run command when server is in following state: {:?}", status.state);
    // }
}
