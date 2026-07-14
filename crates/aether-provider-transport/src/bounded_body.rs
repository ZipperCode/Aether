use flate2::read::{DeflateDecoder, GzDecoder};
use std::io::Read;

pub const RESPONSE_TOO_LARGE_ERROR: &str = "response_too_large";

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum BoundedBodyError {
    #[error("response_too_large")]
    ResponseTooLarge,
    #[error("response body allocation failed")]
    AllocationFailed,
}

#[derive(Debug, thiserror::Error)]
pub enum BoundedBodyReadError {
    #[error(transparent)]
    Bounds(#[from] BoundedBodyError),
    #[error("response body read failed: {0}")]
    Read(#[from] reqwest::Error),
}

#[derive(Debug)]
pub struct BoundedBodyCollector {
    bytes: Vec<u8>,
    limit: usize,
}

impl BoundedBodyCollector {
    pub fn new(content_length: Option<&str>, limit: usize) -> Result<Self, BoundedBodyError> {
        let declared_length = content_length
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .and_then(|value| value.parse::<u64>().ok());
        if declared_length.is_some_and(|length| length > limit as u64) {
            return Err(BoundedBodyError::ResponseTooLarge);
        }

        let initial_capacity = declared_length
            .and_then(|length| usize::try_from(length).ok())
            .unwrap_or(0);
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(initial_capacity)
            .map_err(|_| BoundedBodyError::AllocationFailed)?;
        Ok(Self { bytes, limit })
    }

    pub fn push(&mut self, chunk: &[u8]) -> Result<(), BoundedBodyError> {
        let next_len = self
            .bytes
            .len()
            .checked_add(chunk.len())
            .ok_or(BoundedBodyError::ResponseTooLarge)?;
        if next_len > self.limit {
            return Err(BoundedBodyError::ResponseTooLarge);
        }
        self.bytes
            .try_reserve(chunk.len())
            .map_err(|_| BoundedBodyError::AllocationFailed)?;
        self.bytes.extend_from_slice(chunk);
        Ok(())
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

pub async fn collect_reqwest_body_bounded(
    mut response: reqwest::Response,
    limit: usize,
) -> Result<Vec<u8>, BoundedBodyReadError> {
    let content_length = response
        .headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok());
    let mut body = BoundedBodyCollector::new(content_length, limit)?;
    while let Some(chunk) = response.chunk().await? {
        body.push(&chunk)?;
    }
    Ok(body.into_bytes())
}

pub fn decode_response_body_bounded(
    content_encoding: Option<&str>,
    body: &[u8],
    limit: usize,
) -> Result<Option<Vec<u8>>, BoundedBodyError> {
    let encoding = content_encoding.map(str::trim).unwrap_or_default();
    if encoding.eq_ignore_ascii_case("gzip") {
        return decode_reader_bounded(GzDecoder::new(body), limit);
    }
    if encoding.eq_ignore_ascii_case("deflate") {
        return decode_reader_bounded(DeflateDecoder::new(body), limit);
    }
    Ok(None)
}

fn decode_reader_bounded(
    mut reader: impl Read,
    limit: usize,
) -> Result<Option<Vec<u8>>, BoundedBodyError> {
    let mut decoded = BoundedBodyCollector::new(None, limit)?;
    let mut scratch = [0u8; 8192];
    loop {
        let read = match reader.read(&mut scratch) {
            Ok(read) => read,
            Err(_) => return Ok(None),
        };
        if read == 0 {
            return Ok(Some(decoded.into_bytes()));
        }
        decoded.push(&scratch[..read])?;
    }
}

#[cfg(test)]
mod tests {
    use super::{
        collect_reqwest_body_bounded, decode_response_body_bounded, BoundedBodyCollector,
        BoundedBodyError, BoundedBodyReadError,
    };
    use flate2::{write::DeflateEncoder, write::GzEncoder, Compression};
    use std::io::Write;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn serve_raw_response(response: &'static [u8]) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind raw response server");
        let address = listener.local_addr().expect("raw server address");
        let handle = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept request");
            let mut request = [0_u8; 1024];
            let _ = socket.read(&mut request).await.expect("read request");
            socket.write_all(response).await.expect("write response");
        });
        (format!("http://{address}"), handle)
    }

    #[test]
    fn content_length_over_limit_is_rejected_before_body_collection() {
        // Given / When
        let error = BoundedBodyCollector::new(Some("9"), 8)
            .expect_err("oversized content-length should fail");

        // Then
        assert_eq!(error, BoundedBodyError::ResponseTooLarge);
    }

    #[test]
    fn chunk_crossing_limit_fails_while_exact_limit_succeeds() {
        // Given
        let mut exact = BoundedBodyCollector::new(None, 8).expect("collector");
        let mut oversized = BoundedBodyCollector::new(None, 8).expect("collector");

        // When
        exact.push(b"1234").expect("first exact chunk");
        exact.push(b"5678").expect("exact limit chunk");
        oversized.push(b"12345678").expect("first oversized chunk");
        let error = oversized
            .push(b"9")
            .expect_err("crossing limit should fail");

        // Then
        assert_eq!(exact.into_bytes(), b"12345678");
        assert_eq!(error, BoundedBodyError::ResponseTooLarge);
        assert_eq!(oversized.into_bytes(), b"12345678");
    }

    #[test]
    fn gzip_and_deflate_expansion_over_limit_are_rejected() {
        // Given
        let expanded = vec![b'x'; 9];
        let mut gzip = GzEncoder::new(Vec::new(), Compression::default());
        gzip.write_all(&expanded).expect("gzip input");
        let gzip = gzip.finish().expect("gzip finish");
        let mut deflate = DeflateEncoder::new(Vec::new(), Compression::default());
        deflate.write_all(&expanded).expect("deflate input");
        let deflate = deflate.finish().expect("deflate finish");

        // When / Then
        assert_eq!(
            decode_response_body_bounded(Some("gzip"), &gzip, 8),
            Err(BoundedBodyError::ResponseTooLarge)
        );
        assert_eq!(
            decode_response_body_bounded(Some("deflate"), &deflate, 8),
            Err(BoundedBodyError::ResponseTooLarge)
        );
    }

    #[test]
    fn malformed_encoding_and_content_length_remain_bounded_and_nonfatal() {
        // Given / When
        let mut collector = BoundedBodyCollector::new(Some("not-a-number"), 8)
            .expect("malformed content-length should fall back to chunk bounds");
        collector.push(b"12345678").expect("exact limit");
        let decoded = decode_response_body_bounded(Some("gzip"), b"not-gzip", 8)
            .expect("malformed gzip should preserve existing raw fallback");

        // Then
        assert_eq!(collector.into_bytes(), b"12345678");
        assert_eq!(decoded, None);
    }

    #[tokio::test]
    async fn reqwest_collection_rejects_declared_and_streamed_oversize() {
        // Given
        let (declared_url, declared_server) = serve_raw_response(
            b"HTTP/1.1 200 OK\r\ncontent-length: 9\r\nconnection: close\r\n\r\n123456789",
        )
        .await;
        let (streamed_url, streamed_server) = serve_raw_response(
            b"HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\nconnection: close\r\n\r\n8\r\n12345678\r\n1\r\n9\r\n0\r\n\r\n",
        )
        .await;
        let (small_url, small_server) = serve_raw_response(
            b"HTTP/1.1 200 OK\r\ncontent-length: 5\r\nconnection: close\r\n\r\nsmall",
        )
        .await;

        // When
        let declared = reqwest::get(declared_url).await.expect("declared response");
        let streamed = reqwest::get(streamed_url).await.expect("streamed response");
        let small = reqwest::get(small_url).await.expect("small response");
        let declared_error = collect_reqwest_body_bounded(declared, 8)
            .await
            .expect_err("declared oversize");
        let streamed_error = collect_reqwest_body_bounded(streamed, 8)
            .await
            .expect_err("streamed oversize");
        let small_body = collect_reqwest_body_bounded(small, 8)
            .await
            .expect("small body");
        declared_server.await.expect("declared server");
        streamed_server.await.expect("streamed server");
        small_server.await.expect("small server");

        // Then
        assert!(matches!(
            declared_error,
            BoundedBodyReadError::Bounds(BoundedBodyError::ResponseTooLarge)
        ));
        assert!(matches!(
            streamed_error,
            BoundedBodyReadError::Bounds(BoundedBodyError::ResponseTooLarge)
        ));
        assert_eq!(declared_error.to_string(), "response_too_large");
        assert_eq!(streamed_error.to_string(), "response_too_large");
        assert_eq!(small_body, b"small");
    }
}
