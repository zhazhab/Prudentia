use std::{future::Future, time::Duration};

use reqwest::{RequestBuilder, Response, StatusCode};

use crate::conversation::research::ResearchError;

const RETRY_DELAY: Duration = Duration::from_millis(250);

pub(super) async fn text(request: RequestBuilder) -> Result<String, ResearchError> {
    with_one_retry(request, |request| async move {
        Ok(request.send().await?.error_for_status()?.text().await?)
    })
    .await
}

pub(super) async fn response(request: RequestBuilder) -> Result<Response, ResearchError> {
    with_one_retry(request, |request| async move {
        Ok(request.send().await?.error_for_status()?)
    })
    .await
}

async fn with_one_retry<T, Operation, OperationFuture>(
    request: RequestBuilder,
    operation: Operation,
) -> Result<T, ResearchError>
where
    Operation: Fn(RequestBuilder) -> OperationFuture,
    OperationFuture: Future<Output = Result<T, ResearchError>>,
{
    let retry = request.try_clone();
    match operation(request).await {
        Ok(value) => Ok(value),
        Err(error) if is_retryable(&error) => {
            let Some(retry) = retry else {
                return Err(error);
            };
            tracing::warn!(%error, "public research request failed; retrying once");
            tokio::time::sleep(RETRY_DELAY).await;
            operation(retry).await
        }
        Err(error) => Err(error),
    }
}

fn is_retryable(error: &ResearchError) -> bool {
    let ResearchError::Http(error) = error else {
        return false;
    };
    error.is_timeout()
        || error.is_connect()
        || error.is_body()
        || error.status().is_some_and(|status| {
            status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
        })
}

#[cfg(test)]
mod tests {
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    use super::text;

    #[tokio::test]
    async fn retries_once_after_a_transient_server_error() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let address = listener.local_addr().expect("listener address");
        let server = tokio::spawn(async move {
            for (status, body) in [("500 Internal Server Error", "retry"), ("200 OK", "ok")] {
                let (mut stream, _) = listener.accept().await.expect("connection");
                let mut request = [0_u8; 1024];
                let bytes_read = stream.read(&mut request).await.expect("request");
                assert!(bytes_read > 0);
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream
                    .write_all(response.as_bytes())
                    .await
                    .expect("response");
            }
        });

        let body = text(reqwest::Client::new().get(format!("http://{address}/research")))
            .await
            .expect("retry succeeds");

        assert_eq!(body, "ok");
        server.await.expect("server task");
    }
}
