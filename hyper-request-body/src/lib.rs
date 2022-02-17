pub use hyper::Body as HyperBody;
#[cfg(feature = "warp-request-body")]
pub use warp_request_body;

use core::{
    fmt,
    pin::Pin,
    task::{Context, Poll},
};

use bytes::Bytes;
use futures_util::Stream;
use hyper::Request as HyperRequest;
use pin_project_lite::pin_project;

pub mod error;
mod utils;

use error::Error;

//
pin_project! {
    #[project = BodyProj]
    pub enum Body {
        HyperBody { #[pin] inner: HyperBody },
        Stream { #[pin] inner: Pin<Box<dyn Stream<Item = Result<Bytes, Error>> + Send + 'static>> },
    }
}

impl fmt::Debug for Body {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HyperBody { inner: _ } => write!(f, "HyperBody"),
            Self::Stream { inner: _ } => write!(f, "Stream"),
        }
    }
}

impl fmt::Display for Body {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl Default for Body {
    fn default() -> Self {
        Self::HyperBody {
            inner: HyperBody::default(),
        }
    }
}

//
impl Body {
    pub fn with_hyper_body(hyper_body: HyperBody) -> Self {
        Self::HyperBody { inner: hyper_body }
    }

    pub fn with_stream(stream: impl Stream<Item = Result<Bytes, Error>> + Send + 'static) -> Self {
        Self::Stream {
            inner: Box::pin(stream),
        }
    }

    #[cfg(feature = "warp-request-body")]
    pub fn from_warp_request_body(body: warp_request_body::Body) -> Self {
        use futures_util::TryStreamExt as _;
        use warp_request_body::error::Error as WarpRequestBodyError;

        match body {
            warp_request_body::Body::HyperBody { inner } => Self::with_hyper_body(inner),
            body => Self::with_stream(body.map_err(|err| match err {
                WarpRequestBodyError::HyperError(err) => Error::HyperError(err),
                WarpRequestBodyError::WarpError(err) => Error::Other(err.into()),
            })),
        }
    }

    #[cfg(feature = "warp")]
    // https://github.com/seanmonstar/warp/blob/v0.3.2/src/filters/body.rs#L291
    pub fn from_warp_body_stream(
        stream: impl Stream<Item = Result<impl warp::Buf + 'static, warp::Error>> + Send + 'static,
    ) -> Self {
        use futures_util::TryStreamExt as _;

        // Copy from warp_request_body::utils::buf_to_bytes
        fn buf_to_bytes(mut buf: impl warp::Buf) -> Bytes {
            let mut bytes_mut = bytes::BytesMut::new();
            while buf.has_remaining() {
                bytes_mut.extend_from_slice(buf.chunk());
                let cnt = buf.chunk().len();
                buf.advance(cnt);
            }
            bytes_mut.freeze()
        }

        Self::with_stream(
            stream
                .map_ok(buf_to_bytes)
                .map_err(|err| Error::Other(err.into())),
        )
    }
}

impl From<HyperBody> for Body {
    fn from(body: HyperBody) -> Self {
        Self::with_hyper_body(body)
    }
}

#[cfg(feature = "warp-request-body")]
impl From<warp_request_body::Body> for Body {
    fn from(body: warp_request_body::Body) -> Self {
        Self::from_warp_request_body(body)
    }
}

impl Body {
    pub async fn to_bytes_async(self) -> Result<Bytes, Error> {
        match self {
            Self::HyperBody { inner } => {
                utils::hyper_body_to_bytes(inner).await.map_err(Into::into)
            }
            Self::Stream { inner } => utils::bytes_stream_to_bytes(inner)
                .await
                .map_err(Into::into),
        }
    }
}

//
impl Stream for Body {
    type Item = Result<Bytes, Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.project() {
            BodyProj::HyperBody { inner } => inner.poll_next(cx).map_err(Into::into),
            BodyProj::Stream { inner } => inner.poll_next(cx).map_err(Into::into),
        }
    }
}

//
pub fn hyper_body_request_to_body_request(req: HyperRequest<HyperBody>) -> HyperRequest<Body> {
    let (parts, body) = req.into_parts();
    HyperRequest::from_parts(parts, Body::with_hyper_body(body))
}

pub fn stream_request_to_body_request(
    req: HyperRequest<impl Stream<Item = Result<Bytes, Error>> + Send + 'static>,
) -> HyperRequest<Body> {
    let (parts, body) = req.into_parts();
    HyperRequest::from_parts(parts, Body::with_stream(body))
}

#[cfg(test)]
mod tests {
    use super::*;

    use futures_util::{stream::BoxStream, StreamExt as _, TryStreamExt};

    #[tokio::test]
    async fn test_with_hyper_body() {
        //
        let hyper_body = HyperBody::from("foo");
        let body = Body::with_hyper_body(hyper_body);
        assert!(matches!(body, Body::HyperBody { inner: _ }));
        assert_eq!(
            body.to_bytes_async().await.unwrap(),
            Bytes::copy_from_slice(b"foo")
        );

        //
        let hyper_body = HyperBody::from("foo");
        let mut body = Body::with_hyper_body(hyper_body);
        assert_eq!(
            body.next().await.unwrap().unwrap(),
            Bytes::copy_from_slice(b"foo")
        );
        assert!(body.next().await.is_none());

        //
        let req = HyperRequest::new(HyperBody::from("foo"));
        let (_, body) = hyper_body_request_to_body_request(req).into_parts();
        assert!(matches!(body, Body::HyperBody { inner: _ }));
        assert_eq!(
            body.to_bytes_async().await.unwrap(),
            Bytes::copy_from_slice(b"foo")
        );
    }

    #[tokio::test]
    async fn test_with_stream() {
        //
        let stream =
            futures_util::stream::once(async { Ok(Bytes::copy_from_slice(b"foo")) }).boxed();
        let body = Body::with_stream(stream);
        assert!(matches!(body, Body::Stream { inner: _ }));
        assert_eq!(
            body.to_bytes_async().await.unwrap(),
            Bytes::copy_from_slice(b"foo")
        );

        //
        let stream =
            futures_util::stream::once(async { Ok(Bytes::copy_from_slice(b"foo")) }).boxed();
        let mut body = Body::with_stream(stream);
        assert_eq!(
            body.next().await.unwrap().unwrap(),
            Bytes::copy_from_slice(b"foo")
        );
        assert!(body.next().await.is_none());

        //
        let stream =
            futures_util::stream::once(async { Ok(Bytes::copy_from_slice(b"foo")) }).boxed();
        let req = HyperRequest::new(stream);
        let (_, body) = stream_request_to_body_request(req).into_parts();
        assert!(matches!(body, Body::Stream { inner: _ }));
        assert_eq!(
            body.to_bytes_async().await.unwrap(),
            Bytes::copy_from_slice(b"foo")
        );
    }

    #[cfg(feature = "warp-request-body")]
    #[tokio::test]
    async fn test_from_warp_request_body() {
        //
        let hyper_body = HyperBody::from("foo");
        let warp_body = warp_request_body::Body::with_hyper_body(hyper_body);
        let body = Body::from_warp_request_body(warp_body);
        assert!(matches!(body, Body::HyperBody { inner: _ }));
        assert_eq!(
            body.to_bytes_async().await.unwrap(),
            Bytes::copy_from_slice(b"foo")
        );

        //
        let warp_body = warp_request_body::Body::with_bytes(Bytes::copy_from_slice(b"foo"));
        let body = Body::from_warp_request_body(warp_body);
        assert!(matches!(body, Body::Stream { inner: _ }));
        assert_eq!(
            body.to_bytes_async().await.unwrap(),
            Bytes::copy_from_slice(b"foo")
        );

        //
        let stream =
            futures_util::stream::once(async { Ok(Bytes::copy_from_slice(b"foo")) }).boxed();
        let warp_body = warp_request_body::Body::with_stream(stream);
        let body = Body::from_warp_request_body(warp_body);
        assert!(matches!(body, Body::Stream { inner: _ }));
        assert_eq!(
            body.to_bytes_async().await.unwrap(),
            Bytes::copy_from_slice(b"foo")
        );
    }

    #[cfg(feature = "warp")]
    #[tokio::test]
    async fn test_from_warp_body_stream() {
        //
        let stream = warp::test::request()
            .body("foo")
            .filter(&warp::body::stream())
            .await
            .unwrap();
        let body = Body::from_warp_body_stream(stream);
        assert!(matches!(body, Body::Stream { inner: _ }));
        assert_eq!(
            body.to_bytes_async().await.unwrap(),
            Bytes::copy_from_slice(b"foo")
        );
    }

    pin_project! {
        pub struct BodyWrapper {
            #[pin]
            inner: BoxStream<'static, Result<Bytes, Box<dyn std::error::Error + Send + Sync + 'static>>>
        }
    }
    #[tokio::test]
    async fn test_wrapper() {
        //
        let hyper_body = HyperBody::from("foo");
        let body = Body::with_hyper_body(hyper_body);
        let _ = BodyWrapper {
            inner: body.err_into().boxed(),
        };

        //
        let stream =
            futures_util::stream::once(async { Ok(Bytes::copy_from_slice(b"foo")) }).boxed();
        let body = Body::with_stream(stream);
        let _ = BodyWrapper {
            inner: body.err_into().boxed(),
        };
    }
}
