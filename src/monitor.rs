use std::env; // read env variables
use std::fs::File;
use std::io::BufRead; // read unix socket
use std::io::BufReader; // read unix socket
use std::os::unix::fs::PermissionsExt; // check file permissions
use std::os::unix::net::UnixStream;
use std::process::Command; // execute system command

// listen Hyprland socket with option to pass a callback function
pub fn listen<F>(
    socket_addr: String,
    script_attached: &str,
    script_detached: Option<&str>,
    callback: Option<F>,
) -> std::io::Result<()> 
where
    F: Fn(&str, bool) + 'static,
{
    let stream = match UnixStream::connect(socket_addr) {
        Ok(stream) => stream,
        Err(e) => {
            println!("Couldn't connect: {e:?}");
            return Err(e);
        }
    };
    
    // Skip args check when a callback is provided
    if callback.is_none() {
        let args: Vec<String> = env::args().collect();
        if args.len() < 2 {
            println!("Usage: provide a script to execute.");
            std::process::exit(1);
        }
    }
    
    let mut reader = BufReader::new(stream);
    loop {
        // read message from socket
        let mut buf: Vec<u8> = vec![];
        reader.read_until(b'\n', &mut buf).unwrap();
        let data = String::from_utf8_lossy(&buf);
        let data_parts: Vec<&str> = data.trim().split(">>").collect();
        
        if data_parts.len() < 2 {
            continue;
        }
        
        if data_parts[0] == "monitoradded" {
            if let Some(ref func) = callback {
                // Call the function with monitor id and is_added=true
                func(data_parts[1], true);
            } else {
                // Execute script as before
                // check user has permission to execute script
                let metadata = {
                    let this = File::open(script_attached);
                    match this {
                        Ok(t) => t,
                        Err(_e) => {
                            eprintln!("Error: '{script_attached}' file not found.");
                            continue;
                        }
                    }
                }
                .metadata()
                .unwrap();
                let permissions = metadata.permissions();
                if !permissions.mode() & 0o100 != 0 {
                    eprintln!("Error: '{script_attached}' file is not executable.");
                    continue;
                }
                Command::new(script_attached)
                    .args([data_parts[1]])
                    .spawn()
                    .expect("Failed to execute command");
            }
        } else if data_parts[0] == "monitorremoved" {
            if let Some(ref func) = callback {
                // Call the function with monitor id and is_added=false
                func(data_parts[1], false);
            } else if let Some(script_detached) = script_detached {
                let metadata = {
                    let this = File::open(script_detached);
                    match this {
                        Ok(t) => t,
                        Err(_e) => {
                            eprintln!("Error: '{script_detached}' file not found.");
                            continue;
                        }
                    }
                }
                .metadata()
                .unwrap();
                let permissions = metadata.permissions();
                if !permissions.mode() & 0o100 != 0 {
                    eprintln!("Error: '{script_detached}' file is not executable.");
                    continue;
                }
                Command::new(script_detached)
                    .args([data_parts[1]])
                    .spawn()
                    .expect("Failed to execute command");
            }
        }
    }
}

// Get Hyprland socket path
pub fn get_hyprland_socket() -> Result<String, String> {
    let hypr_inst = env::var("HYPRLAND_INSTANCE_SIGNATURE")
        .map_err(|e| format!("Fatal Error: Hyprland is not running. {}", e))?;

    let default_socket = format!("/tmp/hypr/{}/.socket2.sock", hypr_inst);
    
    // Check if socket is in $XDG_RUNTIME_DIR/hypr first, then fall back
    Ok(match env::var("XDG_RUNTIME_DIR") {
        Ok(runtime_dir) => {
            let path = format!("{}/hypr/{}/.socket2.sock", runtime_dir, hypr_inst);
            if std::fs::metadata(&path).is_ok() {
                path
            } else {
                default_socket
            }
        }
        Err(_) => default_socket,
    })
}

// Note: main function removed as this is now a library module
