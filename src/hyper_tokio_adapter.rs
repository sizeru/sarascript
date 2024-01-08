use std::{
	pin::Pin,
	task::{Context, Poll},
	sync::Arc,
	io::{self, BufReader}, // used for rustls
	fs::File,
};
use hyper::rt::{Read, Write};
use tokio::{
	io::{AsyncRead, AsyncWrite},
	net::TcpStream, task::block_in_place,
};
use crate::{ConfigSettings, error::SaraError};

use tokio_rustls::{client::TlsStream, TlsConnector};
use tokio_rustls::rustls::{pki_types, ClientConfig, RootCertStore};

type Result<T, E=crate::error::SaraError> = std::result::Result<T, E>;

pub enum HyperStream {
	Plain(TcpStream),
	Tls(TlsStream<TcpStream>)
}

impl Read for HyperStream {
	fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, mut buf: hyper::rt::ReadBufCursor<'_>) -> Poll<Result<(), std::io::Error>> {
		// Both TcpStream and TlsStream implement Unpin, which makes this easy.
		// This causes rust to implement Deref in order to transparently access the inner value of the Pin
		// They also implement tokio::io::AsyncRead, so our wrapper only needs to call to that.
		let (poll, bytes_read) = {
			let unfilled = unsafe { buf.as_mut() };
			let ref mut uninit_buf = tokio::io::ReadBuf::uninit(unfilled);
			let poll = match self.get_mut() {
				HyperStream::Plain(ref mut stream) => Pin::new(stream).poll_read(cx, uninit_buf),
				HyperStream::Tls(ref mut tls_stream) => Pin::new(tls_stream).poll_read(cx, uninit_buf),
			};
			// let bytes_read = if let Poll::Ready(Ok(())) = poll { uninit_buf.filled().len() } else { 0 };
			let bytes_read = uninit_buf.filled().len();
			(poll, bytes_read)
		};

		if bytes_read != 0 {
			unsafe { buf.advance(bytes_read) };
		}
		return poll;
	}
}

impl Write for HyperStream {
	fn poll_write( self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<Result<usize, std::io::Error>> {
		match self.get_mut() {
			HyperStream::Plain(ref mut stream) => Pin::new(stream).poll_write(cx, buf),
			HyperStream::Tls(ref mut tls_stream) => Pin::new(tls_stream).poll_write(cx, buf),
		}
	}

	fn poll_flush( self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
		match self.get_mut() {
			HyperStream::Plain(ref mut stream) => Pin::new(stream).poll_flush(cx),
			HyperStream::Tls(ref mut tls_stream) => Pin::new(tls_stream).poll_flush(cx),
		}
	}

	fn poll_shutdown( self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
		match self.get_mut() {
			HyperStream::Plain(ref mut stream) => Pin::new(stream).poll_shutdown(cx),
			HyperStream::Tls(ref mut tls_stream) => Pin::new(tls_stream).poll_shutdown(cx),
		}
	}

}

impl HyperStream {
	pub async fn connect(domain: &str, port: u16, use_tls: bool) -> Result<Self> {
		
		let config = ConfigSettings::get().await?;
		let resolver = block_in_place(|| async {
			hickory_resolver::TokioAsyncResolver::tokio(
				hickory_resolver::config::ResolverConfig::default(),
				hickory_resolver::config::ResolverOpts::default()
			)
		}).await;
		let response = resolver.lookup_ip(domain).await.map_err(|resolve_error| SaraError::DnsResolution(resolve_error))?;
		let address = response.iter().next().unwrap();
		let stream = TcpStream::connect((address, port)).await?;

		if !use_tls {
			return Ok(HyperStream::Plain(stream));
		} else {
			// Let user specify ca_file

			// Read the certificate authority filemove || {
			let root_cert_store = read_certificate_authority_file(&config.certificate_authorities_filename)?;
			let config = ClientConfig::builder()
				.with_root_certificates(root_cert_store)
				.with_no_client_auth(); // i guess this was previously the default?
			let connector = TlsConnector::from(Arc::new(config));

			let domain = pki_types::ServerName::try_from(domain)
				.map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid dnsname"))?
				.to_owned();
	
			let stream = connector.connect(domain, stream).await?;

			return Ok(HyperStream::Tls(stream));
		}
	}
}

pub fn read_certificate_authority_file(ca_filename: &str) -> Result<RootCertStore> {
	let mut rcs = RootCertStore::empty();
	if let Ok(file) = File::open(ca_filename) {
		let mut pem = BufReader::new(file);
		for cert in rustls_pemfile::certs(&mut pem) {
			rcs.add(cert?).unwrap();
		}
	} else {
		rcs.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
	}
	return Ok(rcs);
}