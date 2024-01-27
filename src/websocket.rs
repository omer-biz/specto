use anyhow::{Context, Result};
use futures_util::SinkExt;
use tokio::{
    net::TcpListener,
    sync::{
        mpsc::{self, UnboundedReceiver, UnboundedSender},
        oneshot,
    },
};
use tokio_tungstenite::{accept_async, tungstenite::Message};

pub struct Websocket {
    rx: UnboundedReceiver<Signal>,
    buffer: Vec<Signal>,
}

pub struct Signal {
    msg: &'static str,
    one_tx: oneshot::Sender<&'static str>,
}

impl Websocket {
    pub async fn reload(&mut self) -> Result<()> {
        let _size = self.rx.recv_many(&mut self.buffer, 100).await;

        for signal in self.buffer.drain(..) {
            println!("msg from ws_client: {}", signal.msg);
            signal.one_tx.send("reload").unwrap();
        }

        self.buffer.clear();
        Ok(())
    }
}

pub async fn start_server() -> Result<Websocket> {
    let listener = TcpListener::bind("127.0.0.1:9001")
        .await
        .with_context(|| "Can't bind for websocket listener")?;

    let (tx, rx) = mpsc::unbounded_channel();

    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let tx = tx.clone();
            tokio::spawn(handle_connection(stream, tx));
        }
    });

    Ok(Websocket { rx, buffer: vec![] })
}

async fn handle_connection(
    stream: tokio::net::TcpStream,
    tx: UnboundedSender<Signal>,
) -> Result<()> {
    let mut ws_stream = accept_async(stream)
        .await
        .with_context(|| "Failed to complete websocket handshake")?;

    let (one_tx, one_rx) = oneshot::channel();

    let ready_signal = Signal {
        msg: "ready",
        one_tx,
    };
    tx.send(ready_signal).unwrap();

    let msg = one_rx.await.unwrap();
    // println!("Here 2: {}", msg);
    ws_stream
        .send(Message::Text(msg.to_string()))
        .await
        .unwrap();

    Ok(())
}
