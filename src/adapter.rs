use std::pin::Pin;
use std::task::{Context, Poll};

use async_std::io;
use async_std::net::TcpStream;
use async_std::prelude::*;

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

pub struct HyperStream(pub TcpStream);

impl hyper::rt::Read for HyperStream {
	fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, mut buf: hyper::rt::ReadBufCursor<'_>) -> Poll<Result<(), std::io::Error>> {
		let uninit_slice = unsafe {
			let uninit_buffer = buf.as_mut();
			let ptr = uninit_buffer.as_mut_ptr();
			let ptr: *mut u8 = ptr.cast();
			let size = uninit_buffer.len();
			std::slice::from_raw_parts_mut(ptr, size)
		};

		let read_len = match Pin::new(&mut self.0).poll_read(cx, uninit_slice) {
			Poll::Ready(Ok(len)) => len,
			Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
			Poll::Pending => return Poll::Pending,
		};

		unsafe {
			buf.advance(read_len);
		}
		return Poll::Ready(Ok(()));
	}
}

impl hyper::rt::Write for HyperStream {
	fn poll_write( mut self: Pin<&mut Self>, cx: &mut Context, buf: &[u8]) -> Poll<io::Result<usize>> {
		Pin::new(&mut self.0).poll_write(cx, buf)
	}

	fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<io::Result<()>> {
		Pin::new(&mut self.0).poll_flush(cx)
	}

	fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<io::Result<()>> {
		Pin::new(&mut self.0).poll_close(cx)
	}
}