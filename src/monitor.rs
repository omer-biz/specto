use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use notify::{RecursiveMode, Watcher};
use tokio::{
    sync::mpsc::{self},
    task::JoinHandle,
};

use crate::compiler::Compiler;

pub struct Monitor {
    compiler: Compiler,
}

impl Monitor {
    pub fn new(compiler: Compiler) -> anyhow::Result<Self> {
        Ok(Self { compiler })
    }

    pub fn watch(mut self, path: &Path) -> Result<JoinHandle<Result<()>>> {
        let path = PathBuf::from(path);
        let event_handler = tokio::spawn(async move {
            let (tx, mut rx) = mpsc::unbounded_channel();

            // can't stand on it's own it needs to be in a thread.
            let mut watcher = notify::recommended_watcher(move |event| {
                let _ = tx.send(event);
            })
            .with_context(|| "unable to monitor filesystem")?;

            watcher
                .watch(&path, RecursiveMode::Recursive)
                .with_context(|| format!("unable to watch {:?}", path))?;


            loop {
                if let Some(Ok(event)) = rx.recv().await {
                    // Ignore all other events and recompile if the event is only
                    // Modify -> Data -> Any
                    if let notify::EventKind::Modify(notify::event::ModifyKind::Data(
                        notify::event::DataChange::Any,
                    )) = event.kind
                    {
                        self.compiler.build().await?;
                    }
                }
            }
        });

        Ok(event_handler)
    }
}
