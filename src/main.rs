use std::{
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::Context;
use clap::Parser;
use notify::{RecursiveMode, Watcher};
use tokio::{
    fs::File,
    io::{AsyncReadExt, AsyncWriteExt, BufReader, BufWriter},
    net::TcpListener,
    sync::mpsc,
};

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
    elm_args: Option<Vec<String>>,
}

#[derive(Parser, Debug)]
pub struct ElmArgs {
    /// name of the JS file to output.
    #[arg(default_value = "index.html")]
    output: String,
}

pub struct Compiler {
    command: Command,

}

impl Compiler {
    pub fn new(source: &PathBuf, mut elm_options: Option<Vec<String>>) -> Self {
        let mut command = Command::new("elm");
        command.arg("make").arg(source);

        if let Some(elm_options) = elm_options.take() {
            command.args(&elm_options);
        }

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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = Args::parse();
    let elm_args = args
        .elm_args
        .as_ref()
        .map(|e| ElmArgs::parse_from(e))
        .unwrap_or(ElmArgs::parse());

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

    let output = PathBuf::from(elm_args.output);

    let mut compiler = Compiler::new(&args.source, args.elm_args.take());
    compiler.build()?;

    // watch for changes in elm-source
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut watcher = notify::recommended_watcher(move |e| {
        let _ = tx.send(e);
    })
    .with_context(|| "unable to create watcher")?;
    watcher
        .watch(
            args.source.parent().unwrap_or(Path::new(".")),
            RecursiveMode::Recursive,
        )
        .with_context(|| "unable to watch file")?;

    // monitor file changes
    let _handle = tokio::spawn(async move {
        loop {
            if let Some(Ok(e)) = rx.recv().await {
                // Ignore all other events and recompile if the event is only
                // Modify -> Data -> Any
                if let notify::EventKind::Modify(notify::event::ModifyKind::Data(
                    notify::event::DataChange::Any,
                )) = e.kind
                {
                    let _ = compiler.build();
                }
            }
        }
    });

    // serve the compiled elm file
    let listener = TcpListener::bind(args.address).await?;

    // TODO: if the file is not there recompile to the file we know about
    // which is in `output`
    loop {
        let mut head_buf: Vec<u8> = vec![];
        let mut file_buf: Vec<u8> = vec![];

        // we don't care about the request. gets the same response
        let (mut client, _peer_addr) = listener.accept().await?;
        let (_, writer) = client.split();

        // open the file to be served
        let f = File::open(&output).await?;
        let file_size = f.metadata().await?.len() as usize;
        let mut file_reader = BufReader::new(f);

        // inject websocket client
        let ws_client = include!("../ws_client.txt");
        // building the http header
        let head_len = head_buf
            .write(
                format!(
                    "HTTP/1.1 200 OK\r\n\
        Content-Type: text/html\r\n\
        Content-Length: {}\r\n\
        \r\n",
                    file_size + ws_client.len()
                )
                .as_bytes(),
            )
            .await?;

        // read the file to be served
        let _ = file_reader.read_to_end(&mut file_buf).await?;

        // serve the file
        let mut writer = BufWriter::new(writer);
        writer.write_all(&head_buf[..head_len]).await?;
        writer.write_all(&file_buf[..file_size]).await?;
        writer.write_all(ws_client).await?;
        writer.flush().await?;
    }
}
