use std::path::PathBuf;

use anyhow::Context;
use tokio::process::Command;

pub struct Compiler {
    command: Command,
}

impl Compiler {
    pub fn new(source: &PathBuf, elm_options: Vec<String>) -> Self {
        let mut command = Command::new("elm");
        command.arg("make").arg(source).args(elm_options);

        Self { command }
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

        Ok(())
    }
}
