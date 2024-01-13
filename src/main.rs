use std::{
    fs::File,
    io::Write,
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
    output: PathBuf,
}

impl Compiler {
    pub fn new(source: &PathBuf, elm_options: Option<Vec<String>>) -> (Self, PathBuf) {
        let mut command = Command::new("elm");
        command.arg("make").arg(source);

        let elm_options = elm_options.unwrap_or(vec![]);
        command.args(&elm_options);

        let output_idx = elm_options
            .iter()
            .position(|opt| opt.starts_with("--output"));

        let output = if let Some(index) = output_idx {
            if let Some(output) = elm_options
                .get(index)
                .map(|opt| opt.split("=").nth(1))
                .flatten()
            {
                output
            } else {
                "index.html"
            }
        } else {
            "index.html"
        };

        let output = PathBuf::from(output);

        (
            Self {
                command,
                output: output.clone(),
            },
            output,
        )
    }

    pub fn build(&mut self) -> anyhow::Result<()> {
        let mut child = self
            .command
            .spawn()
            .with_context(|| "can't run `elm-make` command")?;

        let status = child.wait()?;
        if !status.success() {
            println!("Seems like there is a compiler error.\n");
        } else {
            self.inject_websocket_client()?;
        }

        Ok(())
    }

    fn inject_websocket_client(&self) -> anyhow::Result<()> {
        let extra_content = br#"
<script>
console.log("Hello From The Server");
</script>
        "#;
        let mut f = File::options()
            .append(true)
            .open(&self.output)
            .with_context(|| format!("can not open file {:?}", &self.output))?;
        f.write_all(extra_content)?;

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

    let (mut compiler, output) = Compiler::new(&args.source, args.elm_options.take());
    compiler.build()?;

    // watch for changes in elm-source
    let (tx, rx) = mpsc::channel();
    let mut watcher =
        notify::recommended_watcher(tx).with_context(|| "unable to create watcher")?;
    watcher
        .watch(
            args.source.parent().unwrap_or(Path::new(".")),
            RecursiveMode::Recursive,
        )
        .with_context(|| "unable to watch file")?;

    let _handle = thread::spawn(move || -> anyhow::Result<()> {
        for e in rx {
            match e {
                Ok(event) => match event.kind {
                    // Ignore all other events and recompile if the event is only
                    // Modify -> Data -> Any
                    notify::EventKind::Modify(notify::event::ModifyKind::Data(
                        notify::event::DataChange::Any,
                    )) => {
                        compiler.build()?;
                    }
                    _ => (),
                },
                Err(error) => {
                    println!("Got error {:?}", error)
                }
            }
        }

        Ok(())
    });

    let server = Server::http(&args.address).expect("Failed to bind to address");
    for request in server.incoming_requests() {
        let elm_source = File::open(&output).with_context(|| "Failed to open file index.html")?;
        let response = Response::from_file(elm_source);

        request.respond(response)?
    }

    Ok(())
}
