use std::{
    collections::VecDeque,
    fs, io,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    pin::Pin,
    task::{self, Poll},
};

use anyhow::Context as _;
use axum::{http::header, response::IntoResponse};
use futures_core::future::BoxFuture;
use time::UtcDateTime;
use tokio_util::io::ReaderStream;

use crate::{
    Transaction,
    routes::{
        self,
        auth::access_token::{self, AccessToken},
        file,
    },
    utils::hash_to_storage_path,
};

#[expect(clippy::missing_errors_doc)]
pub async fn handler(
    _token: AccessToken<access_token::Manager>,
    trans: Transaction,
) -> routes::Result<impl IntoResponse> {
    let infos = file::Info::get_all_passed(&trans)?;
    trans.commit().await?;

    let ts = UtcDateTime::now().unix_timestamp();
    let bundle_name = ts.to_string();

    let disposition = (
        header::CONTENT_DISPOSITION,
        format!(r#"attachment; filename="{bundle_name}.tar""#),
    );

    let stream = ArchiveStream {
        builder: tar::Builder::new(Vec::with_capacity(1024)),
        header: tar::Header::new_ustar(),
        prefix: PathBuf::from(bundle_name).into(),
        pending: VecDeque::from_iter(infos),
        state: State::NextFile,
    };

    let body = axum::body::Body::from_stream(stream);
    Ok((
        [
            (header::CONTENT_TYPE, "application/x-tar".to_owned()),
            disposition,
        ],
        body,
    ))
}

struct ArchiveStream {
    builder: tar::Builder<Vec<u8>>,
    header: tar::Header,
    prefix: Box<Path>,
    pending: VecDeque<file::Info>,
    state: State,
}

enum State {
    NextFile,
    Metadata {
        fut: BoxFuture<'static, io::Result<fs::Metadata>>,
        path: Box<Path>,
    },
    Open {
        fut: BoxFuture<'static, io::Result<tokio::fs::File>>,
        len: u64,
    },
    Content {
        stream: Pin<Box<ReaderStream<tokio::fs::File>>>,
        len: u64,
    },
    End,
}

impl futures_core::Stream for ArchiveStream {
    type Item = io::Result<Box<[u8]>>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        macro_rules! poll_fut_res {
            ($fut:expr) => {
                match $fut.as_mut().poll(cx) {
                    Poll::Ready(Ok(it)) => it,
                    Poll::Ready(Err(err)) => {
                        self.state = State::End;
                        return Poll::Ready(Some(Err(err)));
                    }
                    Poll::Pending => return Poll::Pending,
                }
            };
        }

        match &mut self.state {
            State::End => Poll::Ready(None),
            State::NextFile => {
                let Some(next) = self.pending.pop_front() else {
                    self.state = State::End;
                    return ready_ok(Box::from([0u8; 1024]));
                };

                let name = next.to_sanitized_file_name();
                let path = self.prefix.join(&*name);
                let path_bytes = path.as_os_str().as_bytes();
                let res = self
                    .builder
                    .append_pax_extensions([("path", path_bytes)]);
                if let Err(err) = res {
                    self.state = State::End;
                    return Poll::Ready(Some(Err(err)));
                }

                let hash =
                    blake3::Hasher::new().update(path_bytes).finalize();
                let res = self.header.set_path(hash.to_hex().as_str());
                if let Err(err) = res {
                    self.state = State::End;
                    return Poll::Ready(Some(Err(err)));
                }

                self.header.set_mode(0o644);
                self.header.set_mtime(
                    next.created_at.unix_timestamp().cast_unsigned(),
                );

                let path = hash_to_storage_path(next.hash);

                let fut = Box::pin(tokio::fs::metadata(path.clone()));
                self.state = State::Metadata { fut, path };

                let buf = self.builder.get_mut();
                let it = Box::from(buf.as_ref());
                buf.clear();
                ready_ok(it)
            }
            State::Metadata { fut, path } => {
                let metadata = poll_fut_res!(fut);
                let path = path.clone();

                let len = metadata.len();
                self.header.set_size(len);
                self.header.set_cksum();

                let fut = Box::pin(async {
                    let file = tokio::fs::OpenOptions::new()
                        .read(true)
                        .open(path)
                        .await?;

                    Ok(file)
                });
                self.state = State::Open { fut, len };

                ready_ok(Box::from(&self.header.as_bytes()[..]))
            }
            State::Open { fut, len } => {
                let file = poll_fut_res!(fut);

                self.state = State::Content {
                    stream: Box::pin(ReaderStream::new(file)),
                    len: *len,
                };
                self.poll_next(cx)
            }
            State::Content { stream, len } => {
                match stream.as_mut().poll_next(cx) {
                    Poll::Ready(Some(Ok(next))) => {
                        ready_ok(Box::<[u8]>::from(next.as_ref()))
                    }
                    Poll::Ready(Some(Err(err))) => {
                        self.state = State::End;
                        Poll::Ready(Some(Err(err)))
                    }
                    Poll::Ready(None) => {
                        const BLOCK_SIZE: usize = 512;
                        const PADDING: [u8; BLOCK_SIZE] = [0; _];

                        let remainder =
                            usize::try_from(*len % BLOCK_SIZE as u64)
                                .expect("usize::MAX >= 512");

                        self.state = State::NextFile;

                        if remainder > 0 {
                            let pad = BLOCK_SIZE - remainder;

                            ready_ok(PADDING[..pad].into())
                        } else {
                            self.poll_next(cx)
                        }
                    }
                    Poll::Pending => Poll::Pending,
                }
            }
        }
    }
}

const fn ready_ok(
    data: Box<[u8]>,
) -> Poll<Option<io::Result<Box<[u8]>>>> {
    Poll::Ready(Some(Ok(data)))
}
