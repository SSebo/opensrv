use std::io::IoSlice;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use pin_project_lite::pin_project;
use tokio::io::{self, AsyncRead, AsyncWrite, ReadBuf, ReadHalf, WriteHalf};
use tokio_rustls::server::TlsStream;
use tokio_rustls::{rustls::ServerConfig, TlsAcceptor};

use crate::packet_reader::PacketReader;
use crate::packet_writer::PacketWriter;
use crate::{AsyncMysqlIntermediary, AsyncMysqlShim, IntermediaryOptions};

pub async fn secure_run_with_options<B1, B2, S, W>(
    mut shim: B1,
    shim2: B2,
    mut reader: S,
    mut writer: W,
    opts: IntermediaryOptions,
    tls_config: Arc<ServerConfig>,
) -> Result<(), B2::Error>
where
    W: AsyncWrite + Send + Unpin,
    B1: AsyncMysqlShim<W> + Send + Sync,
    B2: AsyncMysqlShim<WriteHalf<TlsStream<Duplex<S, W>>>> + Send + Sync,
    S: AsyncRead + Send + Unpin,
{
    let (_, (handshake, seq, auth_context, client_capabilities)) =
        AsyncMysqlIntermediary::init_ssl(
            &mut shim,
            &mut reader,
            &mut writer,
            &Some(tls_config.clone()),
        )
        .await
        .unwrap();
    let (reader, writer) = switch_to_tls(tls_config, reader, writer).await?;
    // let (reader, writer) = packet_wrap(reader, writer);
    let reader = PacketReader::new(reader);
    let writer = PacketWriter::new(writer);

    let process_use_statement_on_query = opts.process_use_statement_on_query;
    let mut mi = AsyncMysqlIntermediary {
        client_capabilities,
        process_use_statement_on_query,
        shim: shim2,
        reader,
        writer,
    };
    mi.init_after_ssl(handshake, seq, auth_context)
        .await
        .unwrap();
    mi.run().await
}

pub async fn switch_to_tls<R: AsyncRead + Send + Unpin, W: AsyncWrite + Send + Unpin>(
    config: Arc<ServerConfig>,
    reader: R,
    writer: W,
) -> std::io::Result<(
    ReadHalf<TlsStream<Duplex<R, W>>>,
    WriteHalf<TlsStream<Duplex<R, W>>>,
)> {
    let stream = Duplex::new(reader, writer);
    let acceptor = TlsAcceptor::from(config);
    let stream = acceptor.accept(stream).await?;
    let (r, w) = tokio::io::split(stream);
    Ok((r, w))
}

pin_project! {
    #[derive(Clone, Debug)]
    pub struct Duplex<R, W> {
        #[pin]
        reader: R,
        #[pin]
        writer: W,
    }
}

impl<R, W> Duplex<R, W> {
    pub fn new(reader: R, writer: W) -> Self {
        Self { reader, writer }
    }
}

impl<R: AsyncRead, W> AsyncRead for Duplex<R, W> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        AsyncRead::poll_read(self.project().reader, cx, buf)
    }
}

impl<R, W: AsyncWrite> AsyncWrite for Duplex<R, W> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        AsyncWrite::poll_write(self.project().writer, cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        AsyncWrite::poll_flush(self.project().writer, cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        AsyncWrite::poll_shutdown(self.project().writer, cx)
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[IoSlice<'_>],
    ) -> Poll<Result<usize, io::Error>> {
        AsyncWrite::poll_write_vectored(self.project().writer, cx, bufs)
    }

    fn is_write_vectored(&self) -> bool {
        AsyncWrite::is_write_vectored(&self.writer)
    }
}
