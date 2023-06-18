// -------------------------------------------------------------------------------------------------
//  Copyright (C) 2015-2023 Nautech Systems Pty Ltd. All rights reserved.
//  https://nautechsystems.io
//
//  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
//  You may not use this file except in compliance with the License.
//  You may obtain a copy of the License at https://www.gnu.org/licenses/lgpl-3.0.en.html
//
//  Unless required by applicable law or agreed to in writing, software
//  distributed under the License is distributed on an "AS IS" BASIS,
//  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
//  See the License for the specific language governing permissions and
//  limitations under the License.
// -------------------------------------------------------------------------------------------------

use std::{io, sync::Arc};

use pyo3::{prelude::*, types::PyBytes, PyObject, Python};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::Mutex,
    task,
};
use tokio_tungstenite::{
    tls::tcp_tls,
    tungstenite::{client::IntoClientRequest, stream::Mode},
    MaybeTlsStream,
};
use tracing::debug;

#[pyclass]
pub struct SocketClient {
    read_task: task::JoinHandle<io::Result<()>>,
    inner: Arc<Mutex<MaybeTlsStream<TcpStream>>>,
    suffix: Box<[u8]>,
}

impl SocketClient {
    pub async fn connect_url(
        url: &str,
        handler: PyObject,
        mode: Mode,
        suffix: Vec<u8>,
    ) -> io::Result<Self> {
        debug!("socket: Connecting to server");
        let stream = TcpStream::connect(url).await?;

        let request = url.into_client_request().unwrap();
        debug!("socket: {:?}", request);
        let inner = Arc::new(Mutex::new(
            tcp_tls(&request, mode, stream, None).await.unwrap(),
        ));
        let reader = inner.clone();

        let suffix_slice = suffix.clone().into_boxed_slice();

        // Keep receiving messages from socket pass them as arguments to handler
        let read_task = task::spawn(async move {
            let mut buf = Vec::new();

            loop {
                let mut locked_reader = reader.lock().await;
                let bytes = locked_reader.read_buf(&mut buf).await?;
                drop(locked_reader);
                debug!("socket: Received {bytes} bytes of data");

                // Terminate if 0 bytes have been read
                // Connection has been terminated or vector buffer is completely
                if bytes == 0 {
                    break;
                } else {
                    // While received data has a line break,
                    // drain and write it to the stream.
                    while let Some((i, _)) = &buf
                        .windows(suffix.len())
                        .enumerate()
                        .find(|(_, pair)| pair.eq(&suffix))
                    {
                        let mut data: Vec<u8> = buf.drain(0..i + suffix.len()).collect();
                        data.truncate(data.len() - suffix.len());

                        Python::with_gil(|py| handler.call1(py, (data.as_slice(),))).unwrap();
                    }
                }
            }
            Ok(())
        });

        Ok(Self {
            read_task,
            inner,
            suffix: suffix_slice,
        })
    }

    /// Shutdown read task and the connection.
    ///
    /// The client must be explicitly shutdown before dropping otherwise
    /// the connection might still be alive for some time before terminating.
    /// Closing the connection is an async call which cannot be done by the
    /// drop method so it must be done explicitly.
    pub async fn shutdown(&mut self) {
        self.read_task.abort();

        let mut inner = self.inner.lock().await;
        inner.shutdown().await.unwrap();
    }

    pub async fn send_bytes(&mut self, data: &[u8]) {
        let mut writer = self.inner.lock().await;
        writer.write_all(data).await.unwrap();
        writer.write_all(&self.suffix).await.unwrap();
    }

    /// Checks if the client is still connected.
    #[inline]
    pub fn is_alive(&self) -> bool {
        !self.read_task.is_finished()
    }
}

#[pymethods]
impl SocketClient {
    #[staticmethod]
    fn connect(
        url: String,
        handler: PyObject,
        ssl: bool,
        suffix: Py<PyBytes>,
        py: Python<'_>,
    ) -> PyResult<&PyAny> {
        let mode = if ssl { Mode::Tls } else { Mode::Plain };
        let suffix = suffix.as_ref(py).as_bytes().to_vec();

        pyo3_asyncio::tokio::future_into_py(py, async move {
            Ok(Self::connect_url(&url, handler, mode, suffix)
                .await
                .unwrap())
        })
    }

    fn send<'py>(slf: PyRef<'_, Self>, data: Vec<u8>, py: Python<'py>) -> PyResult<&'py PyAny> {
        let inner = slf.inner.clone();
        let suffix = slf.suffix.clone();
        pyo3_asyncio::tokio::future_into_py(py, async move {
            let mut writer = inner.lock().await;
            writer.write_all(&data).await?;
            writer.write_all(&suffix).await?;
            Ok(())
        })
    }

    /// Closing the client aborts the reading task and shuts down the connection.
    ///
    /// # Safety
    ///
    /// - The client should not send after being closed
    /// - The client should be dropped after being closed
    fn close<'py>(slf: PyRef<'_, Self>, py: Python<'py>) -> PyResult<&'py PyAny> {
        // cancel reading task
        slf.read_task.abort();

        // Shut down writer
        let inner = slf.inner.clone();
        pyo3_asyncio::tokio::future_into_py(py, async move {
            let mut inner = inner.lock().await;
            inner.shutdown().await.unwrap();
            Ok(())
        })
    }

    fn is_connected(slf: PyRef<'_, Self>) -> bool {
        slf.is_alive()
    }
}

impl Drop for SocketClient {
    fn drop(&mut self) {
        // Cancel reading task
        self.read_task.abort();
    }
}

#[cfg(test)]
mod tests {
    use pyo3::{prelude::*, prepare_freethreaded_python};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
        task::{self, JoinHandle},
        time::{sleep, Duration},
    };
    use tokio_tungstenite::tungstenite::stream::Mode;
    use tracing::debug;
    use tracing_test::traced_test;

    use crate::socket::SocketClient;

    struct TestServer {
        handle: JoinHandle<()>,
        port: u16,
    }

    impl TestServer {
        async fn basic_client_test() -> Self {
            let server = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = TcpListener::local_addr(&server).unwrap().port();

            // Setup test server
            let handle = task::spawn(async move {
                let mut buf = Vec::new();
                let (mut stream, _) = server.accept().await.unwrap();
                debug!("socket:test Server accepted connection");

                loop {
                    let bytes = stream.read_buf(&mut buf).await.unwrap();
                    debug!("socket:test Server received {bytes} bytes");

                    // Terminate if 0 bytes have been read
                    // Connection has been terminated or vector buffer is completely
                    if bytes == 0 {
                        break;
                    } else {
                        // if received data has a line break
                        // extract and write it to the stream
                        while let Some((i, _)) =
                            &buf.windows(2).enumerate().find(|(_, pair)| pair == b"\r\n")
                        {
                            debug!("socket:test Server sending message");
                            stream
                                .write_all(&buf.drain(0..i + 2).as_slice())
                                .await
                                .unwrap();
                        }
                    }
                }
            });

            Self { handle, port }
        }
    }

    #[tokio::test]
    #[traced_test]
    async fn basic_client_test() {
        prepare_freethreaded_python();

        const N: usize = 10;

        // Initialize test server
        let server = TestServer::basic_client_test().await;
        debug!("Reached here");

        // Create counter class and handler that increments it
        let (counter, handler) = Python::with_gil(|py| {
            let pymod = PyModule::from_code(
                py,
                r"
class Counter:
    def __init__(self):
        self.count = 0
        
    def handler(self, bytes):
        if bytes.decode().rstrip() == 'ping':
            self.count = self.count + 1
        
    def get_count(self):
        return self.count

counter = Counter()",
                "",
                "",
            )
            .unwrap();

            let counter = pymod.getattr("counter").unwrap().into_py(py);
            let handler = counter.getattr(py, "handler").unwrap().into_py(py);

            (counter, handler)
        });

        let mut client = SocketClient::connect_url(
            &format!("127.0.0.1:{}", server.port),
            handler.clone(),
            Mode::Plain,
            b"\r\n".to_vec(),
        )
        .await
        .unwrap();

        // Check that socket read task is running
        assert!(client.is_alive());

        // Send messages that increment the count
        for _ in 0..N {
            client.send_bytes(b"ping".as_slice()).await;
        }

        sleep(Duration::from_secs(1)).await;
        // Shutdown client and wait for read task to terminate
        client.shutdown().await;
        server.handle.abort();

        let count_value: usize = Python::with_gil(|py| {
            counter
                .getattr(py, "get_count")
                .unwrap()
                .call0(py)
                .unwrap()
                .extract(py)
                .unwrap()
        });

        // Check count is same as number messages sent
        assert_eq!(count_value, N);
    }
}
