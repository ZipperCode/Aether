use std::{
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use sqlx::{query, Connection, PgConnection};

/// 生命周期测试的自有 PostgreSQL 实例或外部测试服务上的独立数据库。
#[derive(Debug)]
pub(super) struct ManagedPostgresServer {
    child: Option<Child>,
    pg_ctl_bin: PathBuf,
    workdir: PathBuf,
    data_dir: PathBuf,
    database_url: String,
    /// 外部测试服务的管理 URL，仅用于创建及回收本测试数据库，不读取应用配置。
    admin_database_url: Option<String>,
    /// 本夹具创建的独立数据库名；本地进程模式为空。
    owned_database_name: Option<String>,
}

impl ManagedPostgresServer {
    /// 优先使用显式测试 URL 创建独立数据库，否则启动自有本地 PostgreSQL。
    pub(super) async fn try_start() -> Result<Option<Self>, Box<dyn std::error::Error>> {
        if let Some(base_url) = std::env::var("AETHER_TEST_POSTGRES_URL")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
        {
            return match Self::start_from_external_url(&base_url).await {
                Ok(server) => Ok(Some(server)),
                Err(error) if !local_postgres_tests_required() => {
                    eprintln!(
                        "skipping postgres integration test because external postgres is unavailable: {error}"
                    );
                    Ok(None)
                }
                Err(error) => Err(error),
            };
        }
        Self::try_start_local().await
    }

    /// 只启动自有进程；进程关闭回归不允许被外部测试 URL 替代。
    async fn try_start_local() -> Result<Option<Self>, Box<dyn std::error::Error>> {
        let required = local_postgres_tests_required();
        let initdb_bin = configured_binary("AETHER_INITDB_BIN", "initdb");
        let postgres_bin = configured_binary("AETHER_POSTGRES_BIN", "postgres");
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

        if !command_exists(Path::new(&initdb_bin))
            || !command_exists(Path::new(&postgres_bin))
            || !command_exists(&pg_ctl_bin)
        {
            let message = format!(
                "required postgres integration test binaries are unavailable: initdb={initdb_bin}, postgres={postgres_bin}, pg_ctl={}",
                pg_ctl_bin.display()
            );
            if required {
                return Err(std::io::Error::new(std::io::ErrorKind::NotFound, message).into());
            }
            eprintln!("skipping postgres integration test because {message}");
            return Ok(None);
        }

        match Self::start(initdb_bin, postgres_bin, pg_ctl_bin).await {
            Ok(server) => Ok(Some(server)),
            Err(error) if !required && postgres_local_startup_unavailable(&error.to_string()) => {
                eprintln!(
                    "skipping postgres integration test because local postgres could not start in this environment: {error}"
                );
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }

    /// 在显式测试服务创建唯一数据库，避免不同测试共享表或数据。
    async fn start_from_external_url(base_url: &str) -> Result<Self, Box<dyn std::error::Error>> {
        wait_for_postgres(base_url).await?;
        let database_name = format!(
            "aether_migrate_{}_{}",
            std::process::id(),
            &uuid::Uuid::new_v4().simple().to_string()[..12]
        );
        validate_postgres_identifier(&database_name)?;
        let mut database_url = url::Url::parse(base_url)?;
        database_url.set_path(&format!("/{database_name}"));
        let mut admin = PgConnection::connect(base_url).await?;
        query(&format!("CREATE DATABASE \"{database_name}\""))
            .execute(&mut admin)
            .await?;
        let server = Self {
            child: None,
            pg_ctl_bin: PathBuf::new(),
            workdir: PathBuf::new(),
            data_dir: PathBuf::new(),
            database_url: database_url.into(),
            admin_database_url: Some(base_url.to_string()),
            owned_database_name: Some(database_name),
        };
        admin.close().await?;
        wait_for_postgres(server.database_url()).await?;
        Ok(server)
    }

    /// 创建临时数据目录及进程，失败时沿用夹具的停止和清理流程。
    async fn start(
        initdb_bin: String,
        postgres_bin: String,
        pg_ctl_bin: PathBuf,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        drop(listener);
        let workdir = std::env::temp_dir().join(format!(
            "aether-lifecycle-tests-{}-{port}",
            std::process::id()
        ));
        std::fs::create_dir(&workdir)?;
        let mut server = Self {
            child: None,
            pg_ctl_bin,
            data_dir: workdir.join("data"),
            workdir,
            database_url: format!("postgres://aether@127.0.0.1:{port}/postgres"),
            admin_database_url: None,
            owned_database_name: None,
        };

        let init_output = Command::new(&initdb_bin)
            .arg("-D")
            .arg(&server.data_dir)
            .args([
                "-U",
                "aether",
                "--auth=trust",
                "--encoding=UTF8",
                "--no-instructions",
            ])
            .output()?;
        if !init_output.status.success() {
            return Err(std::io::Error::other(format!(
                "initdb failed: {}",
                String::from_utf8_lossy(&init_output.stderr)
            ))
            .into());
        }

        let log_path = server.workdir.join("postgres.log");
        let stdout = std::fs::File::create(&log_path)?;
        let stderr = stdout.try_clone()?;
        server.child = Some(
            Command::new(&postgres_bin)
                .arg("-D")
                .arg(&server.data_dir)
                .args(["-h", "127.0.0.1", "-p"])
                .arg(port.to_string())
                .arg("-F")
                .args(["-c", "unix_socket_directories="])
                .args(["-c", "fsync=off"])
                .args(["-c", "synchronous_commit=off"])
                .args(["-c", "full_page_writes=off"])
                .args(["-c", "shared_buffers=8MB"])
                .args(["-c", "max_connections=8"])
                .args(["-c", "dynamic_shared_memory_type=mmap"])
                .args(["-c", "autovacuum=off"])
                .stdout(Stdio::from(stdout))
                .stderr(Stdio::from(stderr))
                .spawn()?,
        );

        if let Err(error) = wait_for_postgres(&server.database_url).await {
            let logs = std::fs::read_to_string(&log_path).unwrap_or_default();
            server.stop()?;
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!("{error}; postgres logs:\n{logs}"),
            )
            .into());
        }
        Ok(server)
    }

    pub(super) fn database_url(&self) -> &str {
        &self.database_url
    }

    fn stop(&mut self) -> Result<(), std::io::Error> {
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

    /// 析构时在短生命周期线程中回收本测试数据库，避免阻塞当前异步运行时。
    fn drop_owned_database(&mut self) {
        let (Some(admin_url), Some(database_name)) = (
            self.admin_database_url.take(),
            self.owned_database_name.take(),
        ) else {
            return;
        };
        let join = std::thread::spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    eprintln!("failed to build postgres fixture cleanup runtime: {error}");
                    return;
                }
            };
            if let Err(error) =
                runtime.block_on(drop_isolated_postgres_database(&admin_url, &database_name))
            {
                eprintln!(
                    "failed to drop isolated postgres fixture database {database_name}: {error}"
                );
            }
        });
        let _ = join.join();
    }
}

impl Drop for ManagedPostgresServer {
    /// 仅清理本夹具拥有的数据库或已成功停止的本地数据目录。
    fn drop(&mut self) {
        self.drop_owned_database();
        match self.stop() {
            Ok(()) => {
                if !self.workdir.as_os_str().is_empty() {
                    let _ = std::fs::remove_dir_all(&self.workdir);
                }
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

/// PostgreSQL 标识符不能绑定参数；只接受测试生成的字母、数字及下划线名称。
fn validate_postgres_identifier(name: &str) -> Result<(), Box<dyn std::error::Error>> {
    if name.is_empty()
        || !name
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("invalid postgres identifier for test database: {name}"),
        )
        .into());
    }
    Ok(())
}

/// 关闭且删除唯一测试数据库；不会终止其他数据库连接或删除服务实例。
async fn drop_isolated_postgres_database(
    admin_url: &str,
    database_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    validate_postgres_identifier(database_name)?;
    let mut admin = PgConnection::connect(admin_url).await?;
    query("SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = $1 AND pid <> pg_backend_pid()")
        .bind(database_name)
        .execute(&mut admin)
        .await?;
    query(&format!("DROP DATABASE IF EXISTS \"{database_name}\""))
        .execute(&mut admin)
        .await?;
    admin.close().await?;
    Ok(())
}

fn configured_binary(variable: &str, default: &str) -> String {
    std::env::var(variable)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn command_exists(binary: &Path) -> bool {
    if binary.is_absolute() || binary.components().count() > 1 {
        return binary.is_file();
    }
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|path| path.join(binary).is_file()))
}

fn local_postgres_tests_required() -> bool {
    std::env::var("AETHER_REQUIRE_LOCAL_POSTGRES_TESTS")
        .ok()
        .is_some_and(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
}

fn postgres_local_startup_unavailable(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    (message.contains("shared memory")
        && (message.contains("could not create shared memory segment")
            || message.contains("shmget")
            || message.contains("no space left on device")))
        || (message.contains("timed out waiting for local postgres")
            && (message.contains("connection refused")
                || message.contains("os error 61")
                || message.contains("os error 111")))
}

/// 给本地或显式外部测试服务最多 30 秒完成启动，并验证一次真实连接。
async fn wait_for_postgres(database_url: &str) -> Result<(), Box<dyn std::error::Error>> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        match PgConnection::connect(database_url).await {
            Ok(connection) => {
                connection.close().await?;
                return Ok(());
            }
            Err(_) if Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            Err(error) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    format!("timed out waiting for local postgres: {error}"),
                )
                .into());
            }
        }
    }
}

#[tokio::test]
/// 验证自有本地进程即使仍有连接也能停止和回收目录。
async fn managed_postgres_stops_cleanly_with_open_connections() {
    let Some(mut server) = ManagedPostgresServer::try_start_local().await.unwrap() else {
        return;
    };
    let connection = PgConnection::connect(server.database_url()).await.unwrap();
    let workdir = server.workdir.clone();
    assert!(server.data_dir.join("postmaster.pid").exists());
    server.stop().unwrap();
    assert!(server.child.is_none());
    assert!(!server.data_dir.join("postmaster.pid").exists());
    server.stop().unwrap();
    drop(connection);
    drop(server);
    assert!(!workdir.exists());
}

#[tokio::test]
/// 验证停止失败时保留进程所有权，修复命令后可重新停止。
async fn failed_postgres_stop_retains_ownership_for_retry() {
    let Some(mut server) = ManagedPostgresServer::try_start_local().await.unwrap() else {
        return;
    };
    let pg_ctl_bin = server.pg_ctl_bin.clone();
    let workdir = server.workdir.clone();
    server.pg_ctl_bin = workdir.join("missing-pg-ctl");
    assert!(server.stop().is_err());
    assert!(server.child.as_mut().unwrap().try_wait().unwrap().is_none());
    assert!(server.data_dir.exists());
    server.pg_ctl_bin = pg_ctl_bin;
    server.stop().unwrap();
    drop(server);
    assert!(!workdir.exists());
}
