use std::{
    fs::File,
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc,
    thread,
};

use anyhow::Context;
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

    /// options to put after the `elm make` command
    #[arg(last = true, value_name = "<elm-options>")]
    elm_options: Option<Vec<String>>,
}

pub struct Compiler {
    command: Command,
}

impl Compiler {
    pub fn new(source: &PathBuf, elm_options: Option<Vec<String>>) -> Self {
        let mut command = Command::new("elm");
        command.arg("make").arg(source);
        command.args(elm_options.unwrap_or(vec![]));

        Self { command }
    }

    pub fn build(&mut self) -> anyhow::Result<()> {
        let mut child = self
            .command
            .spawn()
            .with_context(|| "can't run `elm-make` command")?;

        let status = child.wait()?;
        if !status.success() {
            println!("Seems like there is a compiler error.\n");
        }

        Ok(())
    }
}

fn main() -> anyhow::Result<()> {
    let mut args = Args::parse();

    // 1. check if the file exists
    // 2. compile it to index.html
    // 3. inject index.html with some websocket stuff
    // 4. serve index.html
    // 5. on another thread interact interact with the websocket client
    // 6. watch the elm file if ther is any change go to 2

    if !args.source.exists() {
        println!("Error: file {:?} not found.", args.source);
        std::process::exit(1);
    }

    let mut compiler = Compiler::new(&args.source, args.elm_options.take());
    compiler.build().with_context(|| "compiling source ...")?;

    // watch for changes in elm-source
    let (tx, rx) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(tx).expect("unable to create watcher");
    watcher
        .watch(
            args.source.parent().unwrap_or(Path::new(".")),
            RecursiveMode::Recursive,
        )
        .expect("unable to watch file");

    thread::spawn(move || {
        for e in rx {
            match e {
                Ok(event) => match event.kind {
                    // Ignore all other events and recompile if the event is only
                    // Modify -> Data -> Any
                    notify::EventKind::Modify(notify::event::ModifyKind::Data(
                        notify::event::DataChange::Any,
                    )) => {
                        let _ = compiler.build();
                    }
                    _ => (),
                },
                Err(error) => {
                    println!("Got error {:?}", error)
                }
            }
        }
    });

    let server = Server::http(&args.address).expect("Failed to bind to address");
    for request in server.incoming_requests() {
        println!("url: {}", request.url());
        let elm_source = File::open("index.html").expect("Failed to open file");
        let response = Response::from_file(elm_source);

        request.respond(response).expect("Failed to send response");
    }
    Ok(())
}
