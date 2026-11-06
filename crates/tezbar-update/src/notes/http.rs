//! Demand-driven exact-tag transport. It has no startup task, retries or detached work.
use futures_util::StreamExt;
use std::{sync::OnceLock, time::Duration};

const MAX_BYTES: usize = 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(10);

fn endpoint(url: &str, accept: &str) -> Result<reqwest::Url, String> {
    let version = url
        .strip_prefix(super::RELEASE_TAG_PREFIX)
        .filter(|version| super::valid_version(version));
    if version.is_none() || accept != super::RELEASE_ACCEPT {
        return Err("Invalid release-notes request.".into());
    }
    reqwest::Url::parse(url).map_err(|_| "Invalid release-notes request.".into())
}

/// Only the exact Tezbar tag route produced by `Notes::open` is accepted.
/// Dropping this future drops its response/body read; nothing is spawned or replayed.
pub async fn fetch(url: &str, accept: &str) -> Result<String, String> {
    let url = endpoint(url, accept)?;
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    if CLIENT.get().is_none() {
        // Cache successful initialization only: a transient TLS/platform failure can recover.
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(TIMEOUT)
            .user_agent(concat!("Tezbar/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|_| "Could not initialize release-notes transport.".to_owned())?;
        let _ = CLIENT.set(client);
    }
    read(
        CLIENT.get().expect("a valid client was installed"),
        url,
        TIMEOUT,
    )
    .await
}

async fn read(
    client: &reqwest::Client,
    url: reqwest::Url,
    timeout: Duration,
) -> Result<String, String> {
    tokio::time::timeout(timeout, async {
        let response = client
            .get(url)
            .header(reqwest::header::ACCEPT, super::RELEASE_ACCEPT)
            .send()
            .await
            .map_err(|_| "Release notes are unavailable right now.")?;
        if !response.status().is_success() {
            return Err(format!(
                "Release notes request failed (HTTP {}).",
                response.status().as_u16()
            ));
        }
        if response
            .content_length()
            .is_some_and(|size| size > MAX_BYTES as u64)
        {
            return Err("Release notes exceed 1 MiB.".into());
        }
        let mut stream = response.bytes_stream();
        let mut bytes = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| "Could not read release notes.")?;
            if chunk.len() > MAX_BYTES - bytes.len() {
                return Err("Release notes exceed 1 MiB.".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        String::from_utf8(bytes).map_err(|_| "Release notes are not valid UTF-8.".into())
    })
    .await
    .map_err(|_| "Release notes request timed out.".to_owned())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn only_the_model_exact_tag_endpoint_and_accept_are_allowed() {
        assert!(endpoint(
            &format!("{}0.3.0-beta.7", super::super::RELEASE_TAG_PREFIX),
            super::super::RELEASE_ACCEPT
        )
        .is_ok());
        for url in [
            "http://api.github.com/repos/almatkai/Tezbar/releases/tags/v1.0.0",
            "https://api.github.com.evil.test/repos/almatkai/Tezbar/releases/tags/v1.0.0",
            "https://api.github.com/repos/almatkai/Tezbar/releases/latest",
            "https://api.github.com/repos/almatkai/Tezbar/releases/tags/v1.0.0?x=1",
            "https://api.github.com/repos/almatkai/Tezbar/releases/tags/v1%2F2",
            "https://api.github.com/repos/almatkai/Tezbar/releases/tags/v",
        ] {
            assert!(
                endpoint(url, super::super::RELEASE_ACCEPT).is_err(),
                "{url}"
            );
        }
        assert!(endpoint(
            &format!("{}1.0.0", super::super::RELEASE_TAG_PREFIX),
            "text/plain"
        )
        .is_err());
    }

    async fn fixture(response: Vec<u8>) -> (reqwest::Url, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = reqwest::Url::parse(&format!(
            "http://{}/fixture",
            listener.local_addr().unwrap()
        ))
        .unwrap();
        let task = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0; 2048];
            let _ = stream.read(&mut request).await;
            let _ = stream.write_all(&response).await;
        });
        (url, task)
    }
    fn client() -> reqwest::Client {
        reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap()
    }
    #[tokio::test]
    async fn response_is_complete_utf8_and_non_success_is_not_a_note() {
        let body = "{\"tag_name\":\"v1.0.0\",\"body\":\"日本 😀\"}";
        let mut response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .into_bytes();
        response.extend_from_slice(body.as_bytes());
        let (url, server) = fixture(response).await;
        assert_eq!(
            read(&client(), url, Duration::from_secs(2)).await.unwrap(),
            body
        );
        server.await.unwrap();
        let (url, server) = fixture(
            b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
        )
        .await;
        assert!(read(&client(), url, Duration::from_secs(2))
            .await
            .unwrap_err()
            .contains("403"));
        server.await.unwrap();
    }
    #[tokio::test]
    async fn declared_and_streamed_oversize_are_bounded() {
        let (url, server) = fixture(
            format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                MAX_BYTES + 1
            )
            .into_bytes(),
        )
        .await;
        assert_eq!(
            read(&client(), url, Duration::from_secs(2))
                .await
                .unwrap_err(),
            "Release notes exceed 1 MiB."
        );
        server.await.unwrap();
        let mut response = b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n".to_vec();
        response.resize(response.len() + MAX_BYTES + 1, b'x');
        let (url, server) = fixture(response).await;
        assert_eq!(
            read(&client(), url, Duration::from_secs(2))
                .await
                .unwrap_err(),
            "Release notes exceed 1 MiB."
        );
        server.await.unwrap();
    }
    #[tokio::test]
    async fn invalid_utf8_is_refused_and_redirects_are_not_followed() {
        let (url, server) = fixture(
            b"HTTP/1.1 200 OK\r\nContent-Length: 1\r\nConnection: close\r\n\r\n\xff".to_vec(),
        )
        .await;
        assert_eq!(
            read(&client(), url, Duration::from_secs(2))
                .await
                .unwrap_err(),
            "Release notes are not valid UTF-8."
        );
        server.await.unwrap();
        let (url, server) = fixture(b"HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:1/not-followed\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec()).await;
        assert!(read(&client(), url, Duration::from_secs(2))
            .await
            .unwrap_err()
            .contains("302"));
        server.await.unwrap();
    }
    #[tokio::test]
    async fn timeout_covers_a_body_that_stops_after_headers() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = reqwest::Url::parse(&format!(
            "http://{}/fixture",
            listener.local_addr().unwrap()
        ))
        .unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0; 2048];
            let _ = stream.read(&mut request).await;
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1\r\n\r\n")
                .await
                .unwrap();
            std::future::pending::<()>().await;
        });
        assert_eq!(
            read(&client(), url, Duration::from_millis(100))
                .await
                .unwrap_err(),
            "Release notes request timed out."
        );
        server.abort();
        assert!(server.await.unwrap_err().is_cancelled());
    }
}
