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

        let elm_options = elm_options.unwrap_or_default();
        command.args(&elm_options);

        let output_idx = elm_options
            .iter()
            .position(|opt| opt.starts_with("--output"));

        // parse the `--output` argument is provided
        let output = if let Some(index) = output_idx {
            if let Some(output) = elm_options.get(index).and_then(|opt| opt.split('=').nth(1)) {
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

    pub async fn build(&mut self) -> anyhow::Result<()> {
        let mut child = self
            .command
            .spawn()
            .with_context(|| "can't run `elm-make` command")?;

        let status = child.wait()?;
        if !status.success() {
            println!("Seems like there is a compiler error.\n");
        } else {
            self.inject_websocket_client().await?;
        }

        Ok(())
    }

    async fn inject_websocket_client(&self) -> anyhow::Result<()> {
        let extra_content = br#"
            <script>
            console.log("Hello From The Server");
            </script>
            "#;
        let mut f = File::options()
            .append(true)
            .open(&self.output)
            .await
            .with_context(|| format!("can not open file {:?}", &self.output))?;

        f.write_all(extra_content).await?;

        Ok(())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
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
    compiler.build().await?;

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
                    let _ = compiler.build().await;
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

        // building the http header
        let head_len = head_buf
            .write(
                format!(
        "HTTP/1.1 200 OK\r\n\
        Content-Type: text/html\r\n\
        Content-Length: {}\r\n\
        \r\n",
                    file_size
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
        writer.flush().await?;
    }
}
