use std::{
    fs::File,
    io::{Read, Write},
    path::Path,
};

fn main() {
    let ws_client = "ws_client.txt";
    println!("cargo:rerun-if-changed=src/{}", ws_client);

    let dest_path = Path::new("./").join(ws_client);
    let mut imported_file = File::create(dest_path).expect("unable to create include client file");

    imported_file
        .write_all(b"br#\"\n")
        .expect("unable to create to file");

    let mut ws_file =
        File::open("websocket_client.html").expect("unable to open websocket client file");

    let mut buf: [u8; 4096] = [0; 4096];
    while let Ok(size) = ws_file.read(&mut buf) {
        if size == 0 {
            break;
        }

        imported_file
            .write_all(&buf[..size])
            .expect("unable to write to file");
    }

    imported_file
        .write_all(b"\"#\n")
        .expect("unable to write to file");
}
