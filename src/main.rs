mod monitor; // import the monitor module
use std::env;
use std::fs::{File, create_dir_all};
use std::io::{self, BufRead, BufReader, Write};
use std::process::Command;
use std::collections::HashMap;
use serde::{Serialize, Deserialize};

/// Maximum number of workspaces to create (10 per monitor)
const MAX_WORKSPACES: usize = 100;

/// Maximum number of monitors to support
const MAX_MONITORS: usize = 10;
const HOME: &str = "/home/suhailali073";

#[derive(Clone, Debug)]
struct WorkspaceMonitorMap {
    workspace: i32,
    monitor: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct MonitorConfig {
    pub monitors: HashMap<String, Monitor>,
}

// Define a struct that matches hyprctl monitors -j output format
#[derive(Deserialize, Debug)]
struct HyprlandMonitor {
    #[serde(rename = "name")]
    name: String,
    #[serde(rename = "id")]
    id: u32,
    #[serde(rename = "width")]
    width: u32,
    #[serde(rename = "height")]
    height: u32,
    #[serde(rename = "refreshRate")]
    refresh_rate: f32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Monitor {
    pub name: String,
    pub id: u32,
    pub height: u32,
    pub width: u32,
    #[serde(rename = "refresh-rate")]
    pub refresh_rate: f32,
}

impl MonitorConfig {
    // Create a new empty monitor configuration
    pub fn new() -> Self {
        MonitorConfig {
            monitors: HashMap::new(),
        }
    }

    // Load the monitor configuration from the file
    pub fn load() -> io::Result<Self> {
        let path = format!("{}/.cache/monitors.json", HOME);
        let file = File::open(&path)?;
        let reader = BufReader::new(file);
        
        serde_json::from_reader(reader)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    // Save the monitor configuration to the file
    pub fn save(&self) -> io::Result<()> {
        let cache_dir = format!("{}/.cache", HOME);
        create_dir_all(&cache_dir)?;
        
        let path = format!("{}/monitors.json", cache_dir);
        let file = File::create(&path)?;
        
        serde_json::to_writer_pretty(file, self)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
    }

    // Update the monitor configuration from hyprland data
    pub fn update_from_hyprland(&mut self) -> io::Result<()> {
        let monitors_json = run_command("hyprctl monitors -j");
        if monitors_json.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::Other, 
                "Failed to get monitor information from hyprctl"
            ));
        }

        // Debug - print the raw JSON
        println!("Raw JSON from hyprctl: {}", monitors_json);

        let mut hyprland_monitors: Vec<HyprlandMonitor> = serde_json::from_str(&monitors_json)
            .map_err(|e| {
                eprintln!("Error parsing monitor JSON: {}", e);
                io::Error::new(io::ErrorKind::InvalidData, e)
            })?;

        // Limit number of monitors to MAX_MONITORS
        if hyprland_monitors.len() > MAX_MONITORS {
            eprintln!("Warning: More than {} monitors detected. Only the first {} will be used.", 
                      MAX_MONITORS, MAX_MONITORS);
            hyprland_monitors.truncate(MAX_MONITORS);
        }

        // Clear existing monitors
        self.monitors.clear();
        
        // Convert from hyprland format to our format
        for hypr_monitor in hyprland_monitors {
            let monitor = Monitor {
                name: hypr_monitor.name,
                id: hypr_monitor.id,
                height: hypr_monitor.height,
                width: hypr_monitor.width,
                refresh_rate: hypr_monitor.refresh_rate,
            };
            
            // Insert with ID as key
            self.monitors.insert(monitor.id.to_string(), monitor);
        }

        Ok(())
    }

    // Get monitor names sorted by ID
    pub fn get_sorted_monitor_names(&self) -> Vec<String> {
        let mut monitor_ids: Vec<u32> = self.monitors.values().map(|m| m.id).collect();
        monitor_ids.sort();
        
        monitor_ids.iter()
            .map(|id| {
                self.monitors.values()
                    .find(|m| m.id == *id)
                    .map(|m| m.name.clone())
                    .unwrap_or_default()
            })
            .collect()
    }
}

// Helper function to get or create monitor config
fn get_monitor_config() -> MonitorConfig {
    match MonitorConfig::load() {
        Ok(config) => config,
        Err(_) => {
            let mut config = MonitorConfig::new();
            if let Err(e) = config.update_from_hyprland() {
                eprintln!("Warning: couldn't update monitor config: {}", e);
            }
            // Try to save the new config
            if let Err(e) = config.save() {
                eprintln!("Warning: couldn't save monitor config: {}", e);
            }
            config
        }
    }
}

fn display_help(program: &str) {
    println!("Usage: {} [option] [workspace_number]", program);
    println!("Options:");
    println!("  -s | --workspace                           Switch workspace");
    println!("  -m | --move                                Move workspace");
    println!("  -m -s | --move --silent                    Move silently to workspace");
    println!("  --monitor                                  Assign workspaces to monitors");
    println!("  --debug-monitors                           Show monitor configuration");
    println!("");
    println!("Configuration Limits:");
    println!("  Maximum workspaces: {}", MAX_WORKSPACES);
    println!("  Maximum monitors: {}", MAX_MONITORS);
    std::process::exit(1);
}


fn run_command(cmd: &str) -> String {
    match Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .output() {
            Ok(output) => String::from_utf8_lossy(&output.stdout).trim().to_string(),
            Err(e) => {
                eprintln!("Failed to execute command '{}': {}", cmd, e);
                String::new()
            }
        }
}

fn parse_workspace_file(path: &str) -> Vec<WorkspaceMonitorMap> {
    match File::open(path) {
        Ok(file) => {
            let reader = BufReader::new(file);
            let mut maps = Vec::new();

            for line in reader.lines() {
                if let Ok(line) = line {
                    if let Some((ws_str, monitor)) = line.strip_prefix("workspace = ").and_then(|l| l.split_once(", monitor:")) {
                        if let Ok(workspace) = ws_str.trim().parse() {
                            maps.push(WorkspaceMonitorMap {
                                workspace,
                                monitor: monitor.trim().to_string(),
                            });
                        }
                    }
                }
            }
            maps
        },
        Err(e) => {
            eprintln!("Failed to open workspace file '{}': {}", path, e);
            Vec::new()
        }
    }
}

// Modified to use the monitor config
fn assign_workspaces(path: &str) -> Option<String> {
    // Get monitor configuration
    let mut monitor_config = get_monitor_config();
    
    // Update with latest information
    if let Err(e) = monitor_config.update_from_hyprland() {
        eprintln!("Error updating monitor configuration: {}", e);
        // Fall back to the old method if updating fails
        let monitors_raw = run_command("hyprctl monitors -j | jq -r '.[].name'");
        let monitors: Vec<String> = monitors_raw.lines().map(|s| s.to_string()).collect();
        
        return assign_workspaces_to_monitors(path, &monitors);
    }
    
    // Save the updated configuration
    if let Err(e) = monitor_config.save() {
        eprintln!("Warning: couldn't save monitor config: {}", e);
    }
    
    // Get sorted monitor names
    let monitor_names = monitor_config.get_sorted_monitor_names();
    
    assign_workspaces_to_monitors(path, &monitor_names)
}

// Helper function to assign workspaces to the specified monitors
fn assign_workspaces_to_monitors(path: &str, monitors: &[String]) -> Option<String> {
    // Ensure we don't exceed MAX_WORKSPACES
    let workspaces_per_monitor = 10;
    let total_workspaces = monitors.len() * workspaces_per_monitor;
    
    if total_workspaces > MAX_WORKSPACES {
        eprintln!("Warning: Would create {} workspaces which exceeds the maximum of {}.", 
                 total_workspaces, MAX_WORKSPACES);
        eprintln!("Only the first {} monitors will be assigned workspaces.", MAX_WORKSPACES / workspaces_per_monitor);
    }
    
    match File::create(path) {
        Ok(mut file) => {
            let mut workspace = 1;
            let max_monitors_to_use = std::cmp::min(monitors.len(), MAX_WORKSPACES / workspaces_per_monitor);

            for monitor in monitors.iter().take(max_monitors_to_use) {
                for _ in 0..workspaces_per_monitor {
                    if workspace > MAX_WORKSPACES {
                        break;
                    }
                    
                    if let Err(e) = writeln!(file, "workspace = {}, monitor:{}", workspace, monitor) {
                        eprintln!("Error writing to workspace file: {}", e);
                        return None;
                    }
                    workspace += 1;
                }
            }

            run_command("hyprctl monitors | grep 'Monitor' | wc -l > /tmp/monitors.txt");
            run_command("hyprctl reload");
            
            println!("Created {} workspaces across {} monitors", workspace - 1, max_monitors_to_use);
            
            // Return the path as an Option<String>
            Some(path.to_string())
        },
        Err(e) => {
            eprintln!("Unable to create workspace file '{}': {}", path, e);
            None
        }
    }
}

fn get_current_workspace() -> i32 {
    run_command("hyprctl activeworkspace -j | jq -r '.id'")
        .parse()
        .unwrap_or(0)
}

fn get_monitor_count() -> i32 {
    run_command("hyprctl monitors -j | jq 'length'")
        .parse()
        .unwrap_or(1)
}

fn get_current_monitor() -> i32 {
    run_command("hyprctl activeworkspace -j | jq -r '.monitorID'")
        .parse()
        .unwrap_or(0)
}

fn move_silent_workspace(workspace: i32, maps: &[WorkspaceMonitorMap]) {
    if workspace <= 0 {
        eprintln!("Invalid workspace number");
        return;
    }

    let targets: Vec<_> = maps
        .iter()
        .filter(|m| m.workspace % 10 == workspace % 10)
        .map(|m| m.workspace)
        .collect();
    
    if targets.is_empty() {
        eprintln!("No matching workspaces found");
        return;
    }

    let mut sorted_targets = targets.clone();
    sorted_targets.sort();

    let mut min_windows = i32::MAX;
    let mut least_populated = sorted_targets[0];

    for ws in &targets {
        let cmd = format!(
            "hyprctl clients -j | jq \"[.[] | select(.workspace.id == {})] | length\"",
            ws
        );
        let count: i32 = run_command(&cmd).parse().unwrap_or(0);
        if count < min_windows {
            min_windows = count;
            least_populated = *ws;
        }
    }

    let cmd = format!("hyprctl dispatch movetoworkspacesilent {}", least_populated);
    run_command(&cmd);
}

fn move_workspace(workspace: i32, maps: &[WorkspaceMonitorMap]) {
    move_silent_workspace(workspace, maps);

    for ws in maps.iter().filter(|m| m.workspace % 10 == workspace % 10) {
        let cmd = format!("hyprctl dispatch workspace {}", ws.workspace);
        run_command(&cmd);
    }
}

fn switch_workspace(workspace: i32, maps: &[WorkspaceMonitorMap]) {
    if workspace <= 0 {
        eprintln!("Invalid workspace number");
        return;
    }

    let current_workspace = get_current_workspace();
    let monitor_count = get_monitor_count();

    let targets: Vec<_> = maps
        .iter()
        .filter(|m| m.workspace % 10 == workspace % 10)
        .map(|m| m.workspace)
        .collect();
    
    if targets.is_empty() {
        eprintln!("No matching workspaces found");
        return;
    }

    if targets.contains(&current_workspace) {
        let next_monitor = (get_current_monitor() + 1) % monitor_count;
        let cmd = format!("hyprctl dispatch focusmonitor {}", next_monitor);
        run_command(&cmd);
        return;
    }

    for ws in &targets {
        let cmd = format!("hyprctl dispatch workspace {}", ws);
        run_command(&cmd);
    }
}

// Let's also add a debug function to inspect the monitor config
fn debug_monitor_config() {
    let mut config = get_monitor_config();
    if let Err(e) = config.update_from_hyprland() {
        eprintln!("Error updating monitor config: {}", e);
        return;
    }
    
    // Print the config in JSON format
    if let Ok(json) = serde_json::to_string_pretty(&config) {
        println!("Monitor config JSON:\n{}", json);
    }
    
    if let Err(e) = config.save() {
        eprintln!("Error saving monitor config: {}", e);
    } else {
        println!("Monitor config saved to ~/.cache/monitors.json");
    }
}

// Add a new option to the main function to debug monitors
fn main() {
    let args: Vec<String> = env::args().collect();
    let config_path = format!("{}/.config/hypr/ws.conf", HOME);

    if args.len() < 2 {
        display_help(&args[0]);
    }

    match args[1].as_str() {
        "-s" | "--workspace" => {
            if args.len() < 3 {
                display_help(&args[0]);
            }
            let maps = parse_workspace_file(&config_path);
            if let Ok(workspace) = args[2].parse::<i32>() {
                switch_workspace(workspace, &maps);
            } else {
                eprintln!("Invalid workspace number: {}", args[2]);
                display_help(&args[0]);
            }
        }
        "-m" | "--move" => {
            if args.len() < 3 {
                display_help(&args[0]);
            }

            let maps = parse_workspace_file(&config_path);

            if args[2] == "-s" || args[2] == "--silent" {
                if args.len() < 4 {
                    display_help(&args[0]);
                }
                if let Ok(workspace) = args[3].parse::<i32>() {
                    move_silent_workspace(workspace, &maps);
                } else {
                    eprintln!("Invalid workspace number: {}", args[3]);
                    display_help(&args[0]);
                }
            } else if let Ok(workspace) = args[2].parse::<i32>() {
                move_workspace(workspace, &maps);
            } else {
                eprintln!("Invalid workspace number: {}", args[2]);
                display_help(&args[0]);
            }
        }
        "--monitor" => {
            // Get Hyprland socket
            let socket = match monitor::get_hyprland_socket() {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("{}", e);
                    std::process::exit(1);
                }
            };

            if args.len() > 2 {
                // User provided scripts as arguments - use direct script execution
                let script_attached = &args[2];
                let script_detached = if args.len() > 3 {
                    Some(args[3].as_str())
                } else {
                    None
                };

                // Call listen with scripts
                if let Err(e) = monitor::listen(socket, script_attached, script_detached, None::<fn(&str, bool)>) {
                    eprintln!("Error listening to Hyprland socket: {}", e);
                    std::process::exit(1);
                }
            } else {
                // No scripts provided - use callback to assign workspaces when monitors change
                let config_path_clone = config_path.clone();
                
                // Create a callback closure that calls assign_workspaces when a monitor is added
                let callback = move |_monitor_id: &str, is_added: bool| {
                    if is_added {
                        println!("Monitor added, reassigning workspaces...");
                        if let Some(path) = assign_workspaces(&config_path_clone) {
                            println!("Workspaces reassigned. Configuration updated at: {}", path);
                        } else {
                            eprintln!("Failed to reassign workspaces");
                        }
                    } else {
                        println!("Monitor removed, reassigning workspaces...");
                        if let Some(path) = assign_workspaces(&config_path_clone) {
                            println!("Workspaces reassigned. Configuration updated at: {}", path);
                        } else {
                            eprintln!("Failed to reassign workspaces");
                        }
                    }
                };

                // Initial configuration
                println!("Initial workspace assignment...");
                if let Some(path) = assign_workspaces(&config_path) {
                    println!("Initial workspace configuration created at: {}", path);
                } else {
                    eprintln!("Failed to create initial workspace configuration");
                    std::process::exit(1);
                }

                // Start monitoring for changes
                println!("Monitoring for display changes...");
                
                // We need a dummy script path because the API requires it, but it won't be used
                let dummy_script = "/dev/null";
                if let Err(e) = monitor::listen(socket, dummy_script, None, Some(callback)) {
                    eprintln!("Error listening to Hyprland socket: {}", e);
                    std::process::exit(1);
                }
            }
        }
        "--debug-monitors" => {
            debug_monitor_config();
        },
        _ => display_help(&args[0]),
    }
}
