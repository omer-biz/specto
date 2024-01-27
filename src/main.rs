use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Parser;
use specto::{compiler::Compiler, monitor};
use tokio::{net::TcpListener, task::JoinHandle};

use axum::{http::header, response::IntoResponse, routing::get, Router};
use tower_http::services::ServeDir;

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

    /// Custome HTMl file to serve
    #[arg(short, long)]
    container: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let elm_list = args.elm_args.unwrap_or_default();

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

    let mut compiler = Compiler::new(&args.source, elm_list);
    compiler.build().await?;

    // watch for changes in elm-source
    let monitor_handler = monitor::watch(compiler, args.source.parent().unwrap_or(Path::new(".")))?;

    let web_handler: JoinHandle<Result<()>> = tokio::spawn(async move {
        let routes = Router::new()
            .route("/ws_client.js", get(serve_ws_client))
            .fallback_service(routes_static());

        let listener = TcpListener::bind(&args.address)
            .await
            .with_context(|| "unable to bind to port")?;

        println!("Listening on {}", args.address);
        axum::serve(listener, routes)
            .await
            .with_context(|| "unable to serve files")?;

        Ok(())
    });

    tokio::select! {
        handler = web_handler => {
            println!("Error with the web server");
            handler??;
        }
        handler = monitor_handler => {
            println!("Error with file monitoring");
            handler??;
        }
    }

    Ok(())
}

async fn serve_ws_client() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/javascript")],
        include_str!("../websocket_client.js"),
    )
}

fn routes_static() -> Router {
    Router::new().nest_service("/", ServeDir::new("./"))
}
