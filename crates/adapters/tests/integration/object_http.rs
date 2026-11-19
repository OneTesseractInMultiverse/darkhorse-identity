use darkhorse_adapters::media::{configuration, objects::Storage};
use darkhorse_application::media::{Asset, Objects};
use darkhorse_domain::{identity::AssetId, profiles::Error};
use sha2::{Digest, Sha256};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    task::JoinHandle,
};

struct Server {
    endpoint: String,
    requests: Arc<Mutex<Vec<String>>>,
    task: Option<JoinHandle<()>>,
}
impl Drop for Server {
    fn drop(&mut self) {
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}
impl Server {
    async fn new(response: &'static [u8], delay_body: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = requests.clone();
        let task = tokio::spawn(async move {
            for _ in 0..12 {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                loop {
                    let mut byte = [0; 1];
                    stream.read_exact(&mut byte).await.unwrap();
                    request.push(byte[0]);
                    assert!(request.len() < 8192);
                    if request.ends_with(b"\r\n\r\n") {
                        break;
                    }
                }
                let headers = String::from_utf8(request).unwrap();
                let length = headers
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().unwrap())
                    })
                    .unwrap_or(0);
                assert!(length <= 1024);
                stream.read_exact(&mut vec![0; length]).await.unwrap();
                captured.lock().unwrap().push(headers);
                if stream.write_all(response).await.is_err() {
                    continue;
                }
                if delay_body {
                    tokio::time::sleep(Duration::from_secs(7)).await;
                }
                let _ = stream.shutdown().await;
            }
        });
        Self {
            endpoint,
            requests,
            task: Some(task),
        }
    }
    fn storage(&self) -> Storage {
        let settings = configuration::load(envbind::MapEnvironment::from_pairs([
            ("DARKHORSE_OBJECTS_ENABLED", "true"),
            ("DARKHORSE_OBJECTS_ENDPOINT", self.endpoint.as_str()),
            ("DARKHORSE_OBJECTS_BUCKET", "media"),
            ("DARKHORSE_OBJECTS_REGION", "us-east-1"),
            ("DARKHORSE_OBJECTS_ACCESS_KEY", "fixture-access"),
            ("DARKHORSE_OBJECTS_SECRET_KEY", "fixture-secret"),
            ("DARKHORSE_OBJECTS_LOCAL_HTTP", "true"),
        ]))
        .unwrap()
        .unwrap();
        Storage::new(settings).unwrap()
    }
    async fn close(mut self) {
        let task = self.task.take().unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        let address = self
            .endpoint
            .trim_start_matches("http://")
            .trim_end_matches('/');
        // The owned listener must be gone before the next fixture starts.
        drop(TcpListener::bind(address).await.unwrap());
    }
}
fn asset() -> Asset {
    Asset {
        id: AssetId::from_u128(1).unwrap(),
        bytes: 3,
        digest: Sha256::digest(b"abc").into(),
    }
}

#[tokio::test]
async fn get_uses_one_signed_request_and_keeps_verified_bytes_in_cache() {
    let server = Server::new(
        b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\nabc",
        false,
    )
    .await;
    let store = server.storage();
    assert_eq!(store.get(&asset()).await.unwrap(), b"abc");
    assert_eq!(store.get(&asset()).await.unwrap(), b"abc");
    let requests = server.requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].starts_with(
        "GET /media/darkhorse/media/00000000000000000000000000000001.png HTTP/1.1\r\n"
    ));
    assert!(requests[0].contains("AWS4-HMAC-SHA256 Credential=fixture-access/"));
    assert!(requests[0].contains("/us-east-1/s3/aws4_request"));
    assert!(requests[0].contains("x-amz-content-sha256:"));
    assert!(!requests[0].contains("fixture-secret"));
    server.close().await;
}

#[tokio::test]
async fn get_rejects_wrong_missing_oversized_truncated_and_corrupt_representations() {
    for response in [
        b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\nabcd".as_slice(),
        b"HTTP/1.1 200 OK\r\nContent-Length: 5242880\r\nConnection: close\r\n\r\n",
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n3\r\nabc\r\n0\r\n\r\n",
        b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\nab",
        b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\nabd",
        b"HTTP/1.1 206 Partial Content\r\nContent-Length: 3\r\nConnection: close\r\n\r\nabc",
    ] {
        let server = Server::new(response, false).await;
        assert_eq!(server.storage().get(&asset()).await, Err(Error::Unavailable));
        assert_eq!(server.requests.lock().unwrap().len(), 1);
        server.close().await;
    }
}

#[tokio::test]
async fn redirects_and_server_errors_are_not_followed_retried_or_collected() {
    for response in [
        b"HTTP/1.1 307 Temporary Redirect\r\nLocation: /elsewhere\r\nContent-Length: 999999999\r\n\r\n".as_slice(),
        b"HTTP/1.1 503 Service Unavailable\r\nTransfer-Encoding: chunked\r\n\r\n",
    ] {
        for method in ["get", "put", "delete"] {
            let server = Server::new(response, true).await;
            let store = server.storage();
            let start = Instant::now();
            let result = match method {
                "put" => store.put(asset().id, b"abc").await,
                "delete" => store.delete(asset().id).await,
                _ => store.get(&asset()).await.map(|_| ()),
            };
            assert_eq!(result, Err(Error::Unavailable));
            assert!(start.elapsed() < Duration::from_secs(3), "error bodies must not be collected");
            assert_eq!(server.requests.lock().unwrap().len(), 1);
            server.close().await;
        }
    }
}

#[tokio::test]
async fn request_deadline_includes_successful_response_body() {
    let server = Server::new(b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\n", true).await;
    let start = Instant::now();
    assert_eq!(
        server.storage().get(&asset()).await,
        Err(Error::Unavailable)
    );
    assert!((Duration::from_secs(4)..Duration::from_secs(7)).contains(&start.elapsed()));
    assert_eq!(server.requests.lock().unwrap().len(), 1);
    server.close().await;
}
