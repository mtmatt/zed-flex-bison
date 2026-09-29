use std::error::Error;

use flex_bison_lsp::server::{capabilities, State};
use lsp_server::{Connection, Message};

fn main() -> Result<(), Box<dyn Error + Sync + Send>> {
    if std::env::args().any(|a| a == "--version") {
        println!("flex-bison-lsp {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let (connection, io_threads) = Connection::stdio();
    connection.initialize(serde_json::to_value(capabilities())?)?;
    let mut state = State::default();
    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req)? {
                    break;
                }
                connection.sender.send(state.handle_request(req).into())?;
            }
            Message::Notification(n) => {
                for out in state.handle_notification(n) {
                    connection.sender.send(out)?;
                }
            }
            Message::Response(_) => {}
        }
    }
    // The writer thread exits only once every sender is gone.
    drop(connection);
    io_threads.join()?;
    Ok(())
}
