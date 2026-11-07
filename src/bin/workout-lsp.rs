//! Workout log language server over stdio: `workout-lsp`
fn main() -> Result<(), workout_syntax::lsp::server::Error> {
    let (connection, io_threads) = lsp_server::Connection::stdio();
    workout_syntax::lsp::server::run(connection)?;
    io_threads.join()?;
    Ok(())
}
