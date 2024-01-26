use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use tokio::{fs, process::Command, io::AsyncWriteExt};

#[derive(Parser, Debug)]
pub struct ElmArgs {
    /// output of the compilation.
    #[arg(default_value = "index.html")]
    output: String,
}

pub struct Compiler {
    command: Command,
    output: PathBuf,
}

impl Compiler {
    pub fn new(source: &PathBuf, elm_options: Vec<String>) -> Self {
        let elm_args = ElmArgs::parse_from(&elm_options);
        let output = PathBuf::from(elm_args.output);

        let mut command = Command::new("elm");
        command.arg("make").arg(source).args(elm_options);

        Self { command, output }
    }

    pub async fn build(&mut self) -> anyhow::Result<()> {
        let mut child = self
            .command
            .spawn()
            .with_context(|| "unable to execute `elm` command.")?;

        let status = child.wait().await?;
        if !status.success() {
            println!("Seems like there was a compiler error. will you fix it?\n");
        }

        // inject websocket client
        append_to_file(&self.output, "\n\n<script src=\"/ws_client.js\"></script>")
            .await
            .with_context(|| format!("Unable to write to file {:?}", self.output))?;


        Ok(())
    }
}

async fn append_to_file(file_path: &PathBuf, text: &str) -> anyhow::Result<()> {
    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(file_path).await?;

    file.write_all(text.as_bytes()).await?;

    Ok(())
}
