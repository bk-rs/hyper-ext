use bytes::{Bytes, BytesMut};
use futures_util::StreamExt as _;
use hyper::{Body as HyperBody, Error as HyperError};

pub async fn hyper_body_to_bytes(mut hyper_body: HyperBody) -> Result<Bytes, HyperError> {
    let mut bytes_mut = BytesMut::new();
    while let Some(bytes) = hyper_body.next().await {
        let bytes = bytes?;
        bytes_mut.extend_from_slice(&bytes[..]);
    }
    Ok(bytes_mut.freeze())
}

pub async fn hyper_body_to_bytes_with_max_length(
    mut hyper_body: HyperBody,
    max_length: usize,
) -> Result<Bytes, HyperError> {
    let mut bytes_mut = BytesMut::new();
    // .take not working
    while let Some(bytes) = hyper_body.next().await {
        let bytes = bytes?;
        bytes_mut.extend_from_slice(&bytes[..]);
        if bytes_mut.len() >= max_length {
            return Ok(bytes_mut.freeze());
        }
    }
    Ok(bytes_mut.freeze())
}

#[cfg(test)]
mod tests {
    use super::*;

    use futures_util::stream;

    #[tokio::test]
    async fn test_hyper_body_to_bytes() {
        let hyper_body = HyperBody::from("foo");
        assert_eq!(
            hyper_body_to_bytes(hyper_body).await.unwrap(),
            Bytes::copy_from_slice(b"foo")
        );
    }

    #[tokio::test]
    async fn test_hyper_body_to_bytes_with_max_length() {
        let hyper_body = HyperBody::from("foobar");
        assert_eq!(
            hyper_body_to_bytes_with_max_length(hyper_body, 3)
                .await
                .unwrap(),
            Bytes::copy_from_slice(b"foobar")
        );

        let chunks: Vec<Result<_, std::io::Error>> = vec![Ok("hello"), Ok(" "), Ok("world")];
        let hyper_body = HyperBody::wrap_stream(stream::iter(chunks));
        assert_eq!(
            hyper_body_to_bytes_with_max_length(hyper_body, 3)
                .await
                .unwrap(),
            Bytes::copy_from_slice(b"hello")
        );

        let chunks: Vec<Result<_, std::io::Error>> = vec![Ok("fo"), Ok("o"), Ok("bar")];
        let hyper_body = HyperBody::wrap_stream(stream::iter(chunks));
        assert_eq!(
            hyper_body_to_bytes_with_max_length(hyper_body, 3)
                .await
                .unwrap(),
            Bytes::copy_from_slice(b"foo")
        );
    }
}
