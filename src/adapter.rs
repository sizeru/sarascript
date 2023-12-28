use std::pin::Pin;
use std::task::{Context, Poll};

use async_std::io;
use async_std::net::TcpStream;
use async_std::prelude::*;
use async_std_openssl::SslStream;

// #[derive(Clone)]
// pub struct HyperExecutor;

// impl<F> hyper::rt::Executor<F> for HyperExecutor
// where
// 		F: Future + Send + 'static,
// 		F::Output: Send + 'static,
// {
// 		fn execute(&self, fut: F) {
// 				task::spawn(fut);
// 		}
// }

// pub struct HyperListener(pub TcpListener);

// impl hyper::server::accept::Accept for HyperListener {
// 		type Conn = HyperStream;
// 		type Error = io::Error;

// 		fn poll_accept(
// 				mut self: Pin<&mut Self>,
// 				cx: &mut Context,
// 		) -> Poll<Option<Result<Self::Conn, Self::Error>>> {
// 				let stream = task::ready!(Pin::new(&mut self.0.incoming()).poll_next(cx)).unwrap()?;
// 				Poll::Ready(Some(Ok(HyperStream(stream))))
// 		}
// }


pub enum HyperStream {
	Plain(TcpStream),
	Tls(SslStream<TcpStream>)
}

impl hyper::rt::Read for HyperStream {
	fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, mut buf: hyper::rt::ReadBufCursor<'_>) -> Poll<Result<(), std::io::Error>> {
		let uninit_slice = unsafe {
			let uninit_buffer = buf.as_mut();
			let ptr = uninit_buffer.as_mut_ptr();
			let ptr: *mut u8 = ptr.cast();
			let size = uninit_buffer.len();
			std::slice::from_raw_parts_mut(ptr, size)
		};

		let read_len = match self.get_mut() {
			HyperStream::Plain(stream) => {
				match Pin::new(stream).poll_read(cx, uninit_slice) {
					Poll::Ready(Ok(len)) => len,
					Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
					Poll::Pending => return Poll::Pending,
				}
			},
			HyperStream::Tls(ssl_stream) => {
				match Pin::new(ssl_stream).poll_read(cx, uninit_slice) {
					Poll::Ready(Ok(len)) => len,
					Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
					Poll::Pending => return Poll::Pending,
				}
			},
		};

		unsafe {
			buf.advance(read_len);
		}

		return Poll::Ready(Ok(()));
	}
}

impl hyper::rt::Write for HyperStream {
	fn poll_write(self: Pin<&mut Self>, cx: &mut Context, buf: &[u8]) -> Poll<io::Result<usize>> {
		return match self.get_mut() {
			HyperStream::Plain(stream) => Pin::new(stream).poll_write(cx, buf),
			HyperStream::Tls(ssl_stream) => Pin::new(ssl_stream).poll_write(cx, buf),
		};
	}

	fn poll_flush(self: Pin<&mut Self>, cx: &mut Context) -> Poll<io::Result<()>> {
		return match self.get_mut() {
			HyperStream::Plain(stream) => Pin::new(stream).poll_flush(cx),
			HyperStream::Tls(ssl_stream) => Pin::new(ssl_stream).poll_flush(cx),
		};
	}

	fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context) -> Poll<io::Result<()>> {
		return match self.get_mut() {
			HyperStream::Plain(stream) => Pin::new(stream).poll_close(cx),
			HyperStream::Tls(ssl_stream) => Pin::new(ssl_stream).poll_close(cx),
		};
	}
}