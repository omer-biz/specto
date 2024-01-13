use std::{
    fs::File,
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc,
    thread,
};

use clap::Parser;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
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

fn compile_source(source: &PathBuf) {
    let mut elm_make = Command::new("elm");
    elm_make.arg("make").arg(source);
    let _output = elm_make.output().expect("Failed to execute a process");
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
    compile_source(&args.source);

    // watch for changes in elm-source
    let (tx, rx) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(tx).expect("unable to create watcher");

    let res = watcher
        .watch(
            &args.source.parent().unwrap_or(Path::new(".")),
            RecursiveMode::Recursive,
        )
        .expect("unable to watch file");
    println!("res: {:?}", res);

    thread::spawn(move || {
        for e in rx {
            match e {
                Ok(event) => {
                    if event.kind.is_create() || event.kind.is_modify() {
                        println!("Got event {:?}, recompiling...", event);
                        compile_source(&args.source);
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
