use std::{
    fs::File,
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc,
    thread,
};

use clap::Parser;
use notify::{RecursiveMode, Watcher};
use tiny_http::{Response, Server};

#[derive(Parser, Debug)]
pub struct Args {
    /// elm source file to watch
    #[arg(default_value = "src/Main.elm", value_name = "<elm-source>")]
    source: PathBuf,

    /// Address to bind to.
    #[arg(short, long, value_name = "ip:port", default_value = "localhost:9000")]
    address: String,
}

pub struct Compiler {
    command: Command,
}

impl Compiler {
    pub fn new(source: &PathBuf) -> Self {
        let mut command = Command::new("elm");
        command.arg("make").arg(source);

        Self { command }
    }

    pub fn build(&mut self) {
        self.command.output().expect("Failed to execute a process");
    }
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // 1. check if the file exists
    // 2. compile it to index.html
    // 3. inject index.html with some websocket stuff
    // 4. serve index.html
    // 5. on another thread interact interact with the websocket client
    // 6. watch the elm file if ther is any change go to 2

    if !args.source.exists() {
        panic!("File: {:?} not found.", args.source);
    }
    let mut compiler = Compiler::new(&args.source);
    compiler.build();

    // watch for changes in elm-source
    let (tx, rx) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(tx).expect("unable to create watcher");
    watcher
        .watch(
            &args.source.parent().unwrap_or(Path::new(".")),
            RecursiveMode::Recursive,
        )
        .expect("unable to watch file");

    thread::spawn(move || {
        for e in rx {
            match e {
                Ok(event) => {
                    if event.kind.is_create() || event.kind.is_modify() {
                        println!("Got event {:?}, recompiling...", event);
                        compiler.build();
                    }
                }
                Err(error) => {
                    println!("Got error {:?}", error)
                }
            }
        }
    });

    let server = Server::http(&args.address).expect("Failed to bind to address");
    println!("Listening on {}", args.address);
    for request in server.incoming_requests() {
        println!("url: {}", request.url());
        let elm_source = File::open("index.html").expect("Failed to open file");
        let response = Response::from_file(elm_source);

        request.respond(response).expect("Failed to send response");
    }
    Ok(())
}
