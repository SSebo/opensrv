use tokio::io::{AsyncRead, AsyncWrite};
use tokio_rustls::{rustls::ServerConfig, TlsAcceptor};

use crate::{duplex::Duplex, packet_reader::PacketReader, packet_writer::PacketWriter};

pub async fn switch_to_tls<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
    config: std::sync::Arc<ServerConfig>,
    reader: &mut PacketReader<R>,
    writer: &mut PacketWriter<W>,
) -> std::io::Result<()> {
    let mut stream = Duplex::new(reader.take().unwrap(), writer.take().unwrap());
    let acceptor = TlsAcceptor::from(config);
    let mut stream = acceptor.accept(stream).await?;
    let (r, w) = tokio::io::split(stream);
    // reader.replace(r);
    // writer.replace(w);
    Ok(())
}
