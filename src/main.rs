#[cfg(target_os = "linux")]
mod fixture;
#[cfg(target_os = "linux")]
mod linux;
mod model;
#[cfg(target_os = "linux")]
mod runner;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run a JSON shutdown profile in a pre-delegated cgroup v2 subtree.
    Run {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        cgroup_parent: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Instrumented workload. Inject this through the wrapper under test.
    Fixture {
        #[arg(long, default_value_t = 150)]
        cleanup_ms: u64,
        #[arg(long, default_value = "normal")]
        behavior: String,
    },
    #[command(hide = true)]
    Launch {
        cgroup: PathBuf,
        socket: PathBuf,
        cwd: PathBuf,
        executable: String,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

fn main() {
    let cli = Cli::parse();
    #[cfg(target_os = "linux")]
    let result = match cli.command {
        Commands::Run {
            config,
            cgroup_parent,
            json,
        } => {
            let report = runner::run(&config, &cgroup_parent);
            let code = report.outcome.exit_code();
            if json {
                println!("{}", serde_json::to_string_pretty(&report).unwrap());
            } else {
                println!("{}", report.human());
            }
            std::process::exit(code);
        }
        Commands::Fixture {
            cleanup_ms,
            behavior,
        } => fixture::run(cleanup_ms, &behavior),
        Commands::Launch {
            cgroup,
            socket,
            cwd,
            executable,
            args,
        } => runner::launch(&cgroup, &socket, &cwd, &executable, &args),
    };
    #[cfg(not(target_os = "linux"))]
    let result: Result<(), Box<dyn std::error::Error>> = {
        let _ = cli;
        Err("ExitScope runtime requires Linux with cgroup v2".into())
    };
    if let Err(e) = result {
        eprintln!("exitscope: {e}");
        std::process::exit(3);
    }
}
