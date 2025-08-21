//! IPC primitives (feature-gated)
//!
//! Minimal scaffold for cross-platform IPC channels used for control and health.
//! Currently implements a Tokio Unix socket server/client on Unix.

#[cfg(unix)]
/// Unix-specific IPC primitives implemented with Tokio Unix sockets.
pub mod unix {
    use std::io;
    use std::path::Path;
    use tokio::net::{UnixListener, UnixStream};

    /// Bind a Unix domain socket at the given path.
    ///
    /// # Errors
    ///
    /// Returns an error if the socket file cannot be removed or if binding to the
    /// provided path fails.
    pub async fn bind<P: AsRef<Path>>(path: P) -> io::Result<UnixListener> {
        // Remove any stale socket
        let _ = tokio::fs::remove_file(path.as_ref()).await;
        UnixListener::bind(path)
    }

    /// Connect to a Unix domain socket at the given path.
    ///
    /// # Errors
    ///
    /// Returns an error if the connection to the provided socket path fails.
    pub async fn connect<P: AsRef<Path>>(path: P) -> io::Result<UnixStream> {
        UnixStream::connect(path).await
    }
}

#[cfg(windows)]
/// Windows-specific IPC primitives (named pipe stubs; to be implemented).
pub mod windows {
    //! Tokio-based Windows named pipe IPC.
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::windows::named_pipe::{
        ClientOptions, NamedPipeClient, NamedPipeServer, ServerOptions,
    };

    /// Create a new named pipe server at the given pipe name (e.g., \\?\pipe\proc-daemon).
    ///
    /// Returns a server handle that can `connect().await` to wait for a client.
    pub fn create_server<S: AsRef<str>>(name: S) -> std::io::Result<NamedPipeServer> {
        ServerOptions::new()
            .first_pipe_instance(true)
            .create(name.as_ref())
    }

    /// Wait asynchronously for a client to connect to the given server instance.
    pub async fn server_connect(server: &NamedPipeServer) -> std::io::Result<()> {
        server.connect().await
    }

    /// Create a new named pipe client and connect to the given pipe name.
    pub async fn connect<S: AsRef<str>>(name: S) -> std::io::Result<NamedPipeClient> {
        ClientOptions::new().open(name.as_ref())
    }

    /// Simple echo handler demonstrating async read/write on a server connection.
    pub async fn echo_once(mut server: NamedPipeServer) -> std::io::Result<()> {
        let mut buf = [0u8; 1024];
        let n = server.read(&mut buf).await?;
        if n > 0 {
            server.write_all(&buf[..n]).await?;
        }
        server.flush().await?;
        Ok(())
    }
}
