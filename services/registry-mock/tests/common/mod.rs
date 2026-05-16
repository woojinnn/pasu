use std::net::TcpListener;
use tempfile::TempDir;

pub struct TestServer {
    pub base_url: String,
    pub _state_dir: TempDir,
    pub _handle: tokio::task::JoinHandle<()>,
}

impl TestServer {
    pub async fn start() -> Self {
        let state_dir = TempDir::new().unwrap();
        std::env::set_var("REGISTRY_STATE", state_dir.path());
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let listener = tokio::net::TcpListener::from_std(listener).unwrap();

        let app = registry_mock::build_app(state_dir.path().to_path_buf()).await;
        let handle = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        Self {
            base_url: format!("http://127.0.0.1:{port}"),
            _state_dir: state_dir,
            _handle: handle,
        }
    }
}
