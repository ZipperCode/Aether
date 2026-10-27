use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static POSTGRES_WORKDIR_SEQ: AtomicU64 = AtomicU64::new(0);

/// 端口从 initdb 完成到 postgres 真正 bind 之间存在空窗，可能被并行用例抢走。
/// 初始启动只在观测到 bind 冲突时换端口重试，这是重试次数上限。
const START_BIND_ATTEMPTS: usize = 5;
const READY_TIMEOUT: Duration = Duration::from_secs(10);
const READY_POLL_INTERVAL: Duration = Duration::from_millis(50);
/// 单次就绪探测（建连 + 核对 data_directory）的超时，防止卡在别人的监听器上。
const READY_CONNECT_TIMEOUT: Duration = Duration::from_millis(500);

use aether_data::driver::postgres::PostgresPoolConfig;
use aether_data::{DataBackends, DataLayerConfig};
use sqlx::{Connection, PgConnection};

#[derive(Debug)]
pub struct ManagedPostgresServer {
    child: Option<Child>,
    postgres_bin: String,
    pg_ctl_bin: PathBuf,
    port: u16,
    workdir: PathBuf,
    data_dir: PathBuf,
    database_url: String,
}

/// 启动失败的结果，附带“是否是端口 bind 冲突”的判定，调用方据此决定换端口还是直接报错。
struct StartupFailure {
    error: Box<dyn std::error::Error>,
    address_in_use: bool,
}

impl From<std::io::Error> for StartupFailure {
    fn from(error: std::io::Error) -> Self {
        Self {
            error: Box::new(error),
            address_in_use: false,
        }
    }
}

fn database_url_for(port: u16) -> String {
    format!("postgres://aether@127.0.0.1:{port}/postgres")
}

impl ManagedPostgresServer {
    pub async fn start() -> Result<Self, Box<dyn std::error::Error>> {
        // 先绑定并持有监听器，直到 spawn 前一刻才释放：initdb 期间端口不会被并行用例抢占。
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        Self::start_with_reservation(port, Some(listener)).await
    }

    /// 初始启动的完整流程。`reservation` 是调用方已持有的端口预留；传 `None` 表示
    /// 端口从一开始就没有保护（仅测试用它模拟 initdb 后端口已被抢占的场景）。
    async fn start_with_reservation(
        mut port: u16,
        mut reservation: Option<std::net::TcpListener>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // pid+port is not unique: cargo test shares one PID, and ephemeral ports
        // are reused after the listener is dropped. Parallel e2e tests then hit
        // create_dir AlreadyExists.
        let seq = POSTGRES_WORKDIR_SEQ.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let workdir = std::env::temp_dir().join(format!(
            "aether-postgres-baseline-{}-{}-{}-{}",
            std::process::id(),
            port,
            seq,
            nanos
        ));
        let data_dir = workdir.join("data");
        std::fs::create_dir(&workdir)?;

        let initdb_bin = std::env::var("AETHER_INITDB_BIN")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "initdb".to_string());
        let postgres_bin = std::env::var("AETHER_POSTGRES_BIN")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "postgres".to_string());
        let pg_ctl_bin = std::env::var("AETHER_PG_CTL_BIN")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(&postgres_bin).with_file_name(if cfg!(windows) {
                    "pg_ctl.exe"
                } else {
                    "pg_ctl"
                })
            });
        let database_url = database_url_for(port);
        let mut server = Self {
            child: None,
            postgres_bin,
            pg_ctl_bin,
            port,
            workdir,
            data_dir,
            database_url,
        };

        let init_output = Command::new(&initdb_bin)
            .arg("-D")
            .arg(&server.data_dir)
            .arg("-U")
            .arg("aether")
            .arg("--auth=trust")
            .arg("--encoding=UTF8")
            .arg("--no-instructions")
            .output()?;
        if !init_output.status.success() {
            return Err(std::io::Error::other(format!(
                "initdb failed: {}",
                String::from_utf8_lossy(&init_output.stderr)
            ))
            .into());
        }

        // 只有观测到端口 bind 冲突才换端口重试；其他启动失败（权限、二进制缺失、初始化错误）
        // 必须原样报错，不能被重试掩盖。
        let mut attempt = 0usize;
        loop {
            if attempt > 0 {
                // 重新预留一个端口，并同步刷新对外公布的 port/URL。
                let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
                port = listener.local_addr()?.port();
                server.port = port;
                server.database_url = database_url_for(port);
                reservation = Some(listener);
            }
            // 越接近 spawn 才释放预留端口，被并行用例抢占的窗口越小。
            drop(reservation.take());

            match server.spawn_and_wait_ready().await {
                Ok(()) => return Ok(server),
                Err(failure) => {
                    if !failure.address_in_use || attempt + 1 >= START_BIND_ATTEMPTS {
                        return Err(failure.error);
                    }
                }
            }
            attempt += 1;
        }
    }

    pub fn database_url(&self) -> &str {
        &self.database_url
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn stop(&mut self) -> Result<(), std::io::Error> {
        let Some(child) = self.child.as_mut() else {
            return Ok(());
        };
        if child.try_wait()?.is_some() {
            self.child = None;
            return Ok(());
        }

        let output = Command::new(&self.pg_ctl_bin)
            .arg("-D")
            .arg(&self.data_dir)
            .args(["stop", "-m", "fast", "-w", "-t", "10"])
            .output()?;
        if !output.status.success() && child.try_wait()?.is_none() {
            return Err(std::io::Error::other(format!(
                "pg_ctl stop failed for {}: {}{}",
                self.data_dir.display(),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            )));
        }
        child.wait()?;
        self.child = None;
        Ok(())
    }

    pub async fn restart(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.stop()?;
        // restart 必须保持端口、URL 和数据目录不变：调用方可能已经缓存了 database_url。
        // 这里不换端口重试，端口被占用时直接报错；只有初始 start 才允许换端口。
        match self.spawn_and_wait_ready().await {
            Ok(()) => Ok(()),
            Err(failure) => Err(failure.error),
        }
    }

    /// 用当前 `port` 启动 postgres 并等待就绪。失败时自行停掉子进程，但不改动
    /// `port`/`database_url`，因此 restart 能保持调用方已缓存的端点不变。
    async fn spawn_and_wait_ready(&mut self) -> Result<(), StartupFailure> {
        let log_path = self.workdir.join("postgres.log");
        let stdout = std::fs::File::create(&log_path)?;
        let stderr = stdout.try_clone()?;
        let child = Command::new(&self.postgres_bin)
            .arg("-D")
            .arg(&self.data_dir)
            .arg("-h")
            .arg("127.0.0.1")
            .arg("-p")
            .arg(self.port.to_string())
            .arg("-F")
            .arg("-c")
            .arg("unix_socket_directories=")
            .arg("-c")
            .arg("fsync=off")
            .arg("-c")
            .arg("synchronous_commit=off")
            .arg("-c")
            .arg("full_page_writes=off")
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr))
            .spawn()?;
        self.child = Some(child);

        if self.wait_for_owned_ready().await? {
            return Ok(());
        }

        let logs = std::fs::read_to_string(&log_path).unwrap_or_default();
        self.stop()?;
        Err(StartupFailure {
            // bind 冲突时 postgres 日志包含 "could not bind IPv4 address ...: Address already in use"。
            address_in_use: logs.to_ascii_lowercase().contains("address already in use"),
            error: std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!("timed out waiting for local postgres; logs:\n{logs}"),
            )
            .into(),
        })
    }

    /// 等待本夹具自己的 postgres 就绪。仅凭端口可连接无法证明服务来自本夹具：
    /// 并行用例可能已经占用同一端口，把别人的实例误认成自己的子进程。因此连接成功后
    /// 还要核对 `current_setting('data_directory')`；子进程提前退出时立即停止等待。
    async fn wait_for_owned_ready(&mut self) -> Result<bool, std::io::Error> {
        let deadline = tokio::time::Instant::now() + READY_TIMEOUT;
        loop {
            let Some(child) = self.child.as_mut() else {
                return Ok(false);
            };
            if child.try_wait()?.is_some() {
                // 子进程已退出（例如 bind 失败），端口上即便有服务也不属于本夹具。
                self.child = None;
                return Ok(false);
            }

            // 端口上可能是并行用例占用的普通监听器：TCP 能连上但永远不完成握手，
            // 因此每个探测都要超时，避免卡死在别人的服务上。
            let owned = tokio::time::timeout(READY_CONNECT_TIMEOUT, async {
                let Ok(mut connection) = PgConnection::connect(&self.database_url).await else {
                    return false;
                };
                let reported: Option<String> =
                    sqlx::query_scalar("SELECT current_setting('data_directory')")
                        .fetch_one(&mut connection)
                        .await
                        .ok();
                let _ = connection.close().await;
                reported.is_some_and(|reported| data_directory_matches(&reported, &self.data_dir))
            })
            .await
            .unwrap_or(false);
            if owned {
                return Ok(true);
            }

            if tokio::time::Instant::now() >= deadline {
                return Ok(false);
            }
            tokio::time::sleep(READY_POLL_INTERVAL).await;
        }
    }
}

impl Drop for ManagedPostgresServer {
    fn drop(&mut self) {
        match self.stop() {
            Ok(()) => {
                let _ = std::fs::remove_dir_all(&self.workdir);
            }
            Err(error) => {
                eprintln!(
                    "failed to stop managed postgres; preserving {}: {error}",
                    self.workdir.display(),
                );
            }
        }
    }
}

pub async fn prepare_aether_postgres_schema(
    database_url: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let config = PostgresPoolConfig {
        database_url: database_url.to_string(),
        ..Default::default()
    };

    let backends = DataBackends::from_config(DataLayerConfig::from_postgres(config))?;
    let pending_migrations = backends
        .prepare_database_for_startup()
        .await?
        .unwrap_or_default();
    if !pending_migrations.is_empty() {
        backends.run_database_migrations().await?;
    }

    Ok(())
}

/// 判断 postgres 报告的 data_directory 是否属于本夹具。报告值可能是符号链接解析前或
/// 解析后的路径（如 macOS 的 /var -> /private/var），统一 canonicalize 后再比较。
fn data_directory_matches(reported: &str, owned: &Path) -> bool {
    match (Path::new(reported).canonicalize(), owned.canonicalize()) {
        (Ok(reported), Ok(owned)) => reported == owned,
        _ => Path::new(reported) == owned,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires local initdb, postgres, and pg_ctl binaries"]
    async fn live_managed_postgres_restarts_cleanly_with_open_connections() {
        let mut server = ManagedPostgresServer::start().await.unwrap();
        let workdir = server.workdir.clone();
        let mut connection = PgConnection::connect(server.database_url()).await.unwrap();
        sqlx::query("CREATE TABLE restart_probe (value INTEGER NOT NULL)")
            .execute(&mut connection)
            .await
            .unwrap();
        sqlx::query("INSERT INTO restart_probe VALUES (42)")
            .execute(&mut connection)
            .await
            .unwrap();

        for _iteration in 0..4 {
            server.stop().unwrap();
            server.stop().unwrap();
            assert!(server.child.is_none());
            assert!(!server.data_dir.join("postmaster.pid").exists());
            assert!(server.data_dir.exists());
            server.restart().await.unwrap();
            connection = PgConnection::connect(server.database_url()).await.unwrap();
            let value: i32 = sqlx::query_scalar("SELECT value FROM restart_probe")
                .fetch_one(&mut connection)
                .await
                .unwrap();
            assert_eq!(value, 42);
        }

        drop(server);
        assert!(!workdir.exists());
    }

    #[tokio::test]
    #[ignore = "requires local initdb, postgres, and pg_ctl binaries"]
    async fn live_failed_postgres_stop_can_be_retried_without_losing_ownership() {
        let mut server = ManagedPostgresServer::start().await.unwrap();
        let pg_ctl_bin = server.pg_ctl_bin.clone();
        server.pg_ctl_bin = server.workdir.join("missing-pg-ctl");
        assert!(server.stop().is_err());
        assert!(server.child.as_mut().unwrap().try_wait().unwrap().is_none());
        assert!(server.data_dir.exists());
        server.pg_ctl_bin = pg_ctl_bin;
        server.stop().unwrap();
        assert!(server.child.is_none());
        assert!(!server.data_dir.join("postmaster.pid").exists());
    }

    #[tokio::test]
    #[ignore = "requires local initdb, postgres, and pg_ctl binaries"]
    async fn live_managed_postgres_start_recovers_from_occupied_initial_port() {
        // 占用一个端口并保持监听，模拟端口在 initdb 完成到 postgres bind 之间被并行用例抢走。
        let occupied = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let occupied_port = occupied.local_addr().unwrap().port();

        // 传 None 表示初始端口从一开始就没有预留，首次 spawn 必然撞上被占用的端口。
        let server = ManagedPostgresServer::start_with_reservation(occupied_port, None)
            .await
            .unwrap();
        assert_ne!(server.port(), occupied_port);
        assert_eq!(
            server.database_url(),
            database_url_for(server.port()).as_str()
        );
        let workdir = server.workdir.clone();
        assert!(server.data_dir.exists());

        // 校验连接到的确实是本夹具的 data_directory，而不是占用旧端口的另一个实例。
        let mut connection = PgConnection::connect(server.database_url()).await.unwrap();
        let data_directory: String = sqlx::query_scalar("SELECT current_setting('data_directory')")
            .fetch_one(&mut connection)
            .await
            .unwrap();
        assert!(data_directory_matches(&data_directory, &server.data_dir));

        sqlx::query("CREATE TABLE start_collision_probe (value INTEGER NOT NULL)")
            .execute(&mut connection)
            .await
            .unwrap();
        sqlx::query("INSERT INTO start_collision_probe VALUES (7)")
            .execute(&mut connection)
            .await
            .unwrap();
        let value: i32 = sqlx::query_scalar("SELECT value FROM start_collision_probe")
            .fetch_one(&mut connection)
            .await
            .unwrap();
        assert_eq!(value, 7);

        drop(connection);
        drop(occupied);
        drop(server);
        assert!(!workdir.exists());
    }

    #[tokio::test]
    #[ignore = "requires local initdb, postgres, and pg_ctl binaries"]
    async fn live_managed_postgres_restart_fails_without_changing_published_endpoint() {
        let mut server = ManagedPostgresServer::start().await.unwrap();
        let workdir = server.workdir.clone();
        let original_port = server.port();
        // 先占用一个端口再停服务：此时已发布端口仍在监听，新端口必然与其不同。
        let occupied = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let occupied_port = occupied.local_addr().unwrap().port();
        assert_ne!(occupied_port, original_port);

        let mut connection = PgConnection::connect(server.database_url()).await.unwrap();
        sqlx::query("CREATE TABLE restart_endpoint_probe (value INTEGER NOT NULL)")
            .execute(&mut connection)
            .await
            .unwrap();
        sqlx::query("INSERT INTO restart_endpoint_probe VALUES (11)")
            .execute(&mut connection)
            .await
            .unwrap();
        drop(connection);
        server.stop().unwrap();

        // restart 不允许像 start 那样换端口重试：端口被占用时必须原样报错。
        server.port = occupied_port;
        server.database_url = database_url_for(occupied_port);
        let published_url = server.database_url.clone();

        let error = server.restart().await.unwrap_err();
        let message = error.to_string().to_ascii_lowercase();
        assert!(
            message.contains("address already in use"),
            "unexpected restart failure: {error}"
        );
        assert_eq!(server.port(), occupied_port);
        assert_eq!(server.database_url, published_url);
        assert!(server.child.is_none());
        assert!(server.data_dir.exists());

        // 端口释放后 restart 用同一端点恢复，数据目录保持完好。
        drop(occupied);
        server.restart().await.unwrap();
        assert_eq!(server.port(), occupied_port);
        assert_eq!(server.database_url, published_url);
        let mut connection = PgConnection::connect(server.database_url()).await.unwrap();
        let value: i32 = sqlx::query_scalar("SELECT value FROM restart_endpoint_probe")
            .fetch_one(&mut connection)
            .await
            .unwrap();
        assert_eq!(value, 11);

        drop(connection);
        drop(server);
        assert!(!workdir.exists());
    }
}
