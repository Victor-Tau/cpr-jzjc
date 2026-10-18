use crate::host::fault;
use gateway_plugin_sdk::PluginFault;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::{
    io::Read,
    path::{Path, PathBuf},
    process::Stdio,
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

struct Worker {
    child: tokio::process::Child,
    stdin: tokio::process::ChildStdin,
    stdout: BufReader<tokio::process::ChildStdout>,
}

pub struct Engine {
    root: PathBuf,
    directory: Option<tempfile::TempDir>,
    worker: Option<Worker>,
}
impl Engine {
    pub fn cleanup(&mut self) -> Result<(), PluginFault> {
        if let Some(mut worker) = self.worker.take() {
            let _ = worker.child.start_kill();
        }
        if let Some(directory) = self.directory.take() {
            directory
                .close()
                .map_err(|_| fault("评分临时目录清理失败"))?;
        }
        self.root.clear();
        Ok(())
    }
    pub fn new() -> Self {
        Self {
            root: PathBuf::new(),
            directory: None,
            worker: None,
        }
    }
    pub fn prepare(&mut self) -> Result<(), PluginFault> {
        if self.directory.is_some() {
            return Ok(());
        }
        #[cfg(debug_assertions)]
        if std::env::var_os("MODELTRACE_DEV_PYTHON").is_some() {
            self.root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .to_path_buf();
            self.directory = Some(tempfile::tempdir().map_err(|_| fault("评分临时目录创建失败"))?);
            return Ok(());
        }
        let root = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent()?.parent().map(PathBuf::from))
            .ok_or_else(|| fault("插件资源目录不可用"))?;
        let archive = root.join("runtime/runtime.tar.gz");
        if !archive.is_file() {
            return Err(fault("评分运行时缺失，请安装完整插件包"));
        }
        let mut candidates = vec![];
        if let Some(configured) = std::env::var_os("MODELTRACE_RUNTIME_DIR") {
            candidates.push(PathBuf::from(configured));
        }
        candidates.push(std::env::temp_dir());
        candidates.push(PathBuf::from("/var/tmp"));
        let mut tried = std::collections::BTreeSet::new();
        let mut errors = vec![];
        for base in candidates {
            if !tried.insert(base.clone()) {
                continue;
            }
            if std::fs::read_to_string("/proc/self/mountinfo")
                .map(|mounts| mount_disallows_execution(&base, &mounts))
                .unwrap_or(false)
            {
                errors.push(format!("{}：挂载为 noexec，跳过", base.display()));
                continue;
            }
            let directory = match tempfile::Builder::new()
                .prefix("modeltrace-runtime-")
                .tempdir_in(&base)
            {
                Ok(directory) => directory,
                Err(error) => {
                    errors.push(
                        runtime_io_fault(
                            &format!("{}：评分临时目录创建失败", base.display()),
                            &error,
                        )
                        .message,
                    );
                    continue;
                }
            };
            let file = std::fs::File::open(&archive)
                .map_err(|_| fault("评分运行时缺失，请安装完整插件包"))?;
            match unpack(file, directory.path()) {
                Ok(()) => {
                    self.root = directory.path().to_path_buf();
                    self.directory = Some(directory);
                    return Ok(());
                }
                Err(error) => errors.push(format!("{}：{}", base.display(), error.message)),
            }
        }
        Err(fault(format!(
            "评分运行时准备失败：{}；可设置 MODELTRACE_RUNTIME_DIR 为可写、可执行且至少有100MB空闲的目录",
            errors.join("；")
        )))
    }
    async fn prepared(&mut self) -> Result<(), PluginFault> {
        if self.directory.is_some() {
            return Ok(());
        }
        let mut engine = std::mem::replace(self, Self::new());
        let (engine, result) = tokio::task::spawn_blocking(move || {
            let result = engine.prepare();
            (engine, result)
        })
        .await
        .map_err(|_| fault("评分运行时准备任务失败"))?;
        *self = engine;
        result
    }
    pub async fn write_bank(&mut self, hash: &str, data: &[u8]) -> Result<PathBuf, PluginFault> {
        if hash.len() != 64 || !hash.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err(fault("指纹库摘要无效"));
        }
        self.prepared().await?;
        let path = self
            .directory
            .as_ref()
            .unwrap()
            .path()
            .join(format!("bank-{hash}.json"));
        if !path.exists() {
            std::fs::write(&path, data).map_err(|_| fault("指纹库临时加载失败"))?;
        }
        Ok(path)
    }
    pub async fn call<I: Serialize, O: DeserializeOwned>(
        &mut self,
        input: I,
    ) -> Result<O, PluginFault> {
        self.prepared().await?;
        if self.worker.is_none() {
            let mut command;
            #[cfg(debug_assertions)]
            let dev = std::env::var_os("MODELTRACE_DEV_PYTHON");
            #[cfg(not(debug_assertions))]
            let dev: Option<std::ffi::OsString> = None;
            if let Some(python) = dev {
                command = tokio::process::Command::new(python);
            } else {
                command = tokio::process::Command::new(
                    self.root
                        .join("usr/lib/x86_64-linux-gnu/ld-linux-x86-64.so.2"),
                );
                command
                    .arg("--library-path")
                    .arg(format!(
                        "{}/usr/local/lib:{}/usr/lib/x86_64-linux-gnu",
                        self.root.display(),
                        self.root.display()
                    ))
                    .arg(self.root.join("usr/local/bin/python3.11"));
                command
                    .env_clear()
                    .env("PYTHONHOME", self.root.join("usr/local"));
            }
            command
                .arg("-B")
                .arg(self.root.join("engine/worker.py"))
                .env("OPENBLAS_NUM_THREADS", "1")
                .env("OMP_NUM_THREADS", "1")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .kill_on_drop(true);
            let mut child = command.spawn().map_err(|_| fault("无法启动评分进程"))?;
            let stdin = child.stdin.take().unwrap();
            let stdout = BufReader::new(child.stdout.take().unwrap());
            self.worker = Some(Worker {
                child,
                stdin,
                stdout,
            });
        }
        let mut bytes = serde_json::to_vec(&input).map_err(|_| fault("评分输入编码失败"))?;
        bytes.push(b'\n');
        let mut worker = self.worker.take().unwrap();
        let result = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            worker
                .stdin
                .write_all(&bytes)
                .await
                .map_err(|_| fault("评分进程输入失败"))?;
            worker
                .stdin
                .flush()
                .await
                .map_err(|_| fault("评分进程输入失败"))?;
            let mut output = vec![];
            loop {
                let chunk = worker
                    .stdout
                    .fill_buf()
                    .await
                    .map_err(|_| fault("评分结果读取失败"))?;
                if chunk.is_empty() {
                    return Err(fault("评分进程提前结束"));
                }
                let end = chunk.iter().position(|&b| b == b'\n').map(|i| i + 1);
                let count = end.unwrap_or(chunk.len());
                if output.len() + count > 2 * 1024 * 1024 {
                    return Err(fault("评分输出超限"));
                }
                output.extend_from_slice(&chunk[..count]);
                worker.stdout.consume(count);
                if end.is_some() {
                    break;
                }
            }
            let v: Value =
                serde_json::from_slice(&output).map_err(|_| fault("评分输出格式错误"))?;
            if v["ok"] != true {
                return Err(fault(v["error"].as_str().unwrap_or("评分失败")));
            }
            serde_json::from_value(v["data"].clone()).map_err(|_| fault("评分数据格式错误"))
        })
        .await;
        match result {
            Ok(Ok(value)) => {
                self.worker = Some(worker);
                Ok(value)
            }
            other => {
                let _ = worker.child.kill().await;
                match other {
                    Ok(Err(error)) => Err(error),
                    _ => Err(fault("离线评分超过5秒")),
                }
            }
        }
    }
}
fn unpack<R: Read>(file: R, root: &Path) -> Result<(), PluginFault> {
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(file));
    // Some RS deployments deny timestamp syscalls in the plugin sandbox.
    // The scoring runtime does not depend on archive mtimes.
    archive.set_preserve_mtime(false);
    let mut total = 0;
    for entry in archive.entries().map_err(|_| fault("评分运行包损坏"))? {
        let mut e = entry.map_err(|_| fault("评分运行包损坏"))?;
        total += e.size();
        if total > 400 * 1024 * 1024
            || (!e.header().entry_type().is_file() && !e.header().entry_type().is_dir())
        {
            return Err(fault("评分运行包包含不支持的条目"));
        }
        let path = e
            .path()
            .map_err(|_| fault("评分运行包路径无效"))?
            .into_owned();
        if !e.unpack_in(root).map_err(|error| {
            runtime_io_fault(
                &format!("评分运行包解压失败（条目 {}）", path.display()),
                &error,
            )
        })? {
            return Err(fault("评分运行包路径无效"));
        }
    }
    Ok(())
}

fn runtime_io_fault(action: &str, error: &std::io::Error) -> PluginFault {
    let mut cause: &dyn std::error::Error = error;
    while let Some(source) = cause.source() {
        cause = source;
    }
    fault(format!(
        "{action}：{cause}；请检查临时目录可用空间和写入权限（可用 TMPDIR 指定目录）"
    ))
}

fn mount_disallows_execution(path: &Path, mounts: &str) -> bool {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    mounts
        .lines()
        .filter_map(|line| {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            let mount = fields
                .get(4)?
                .replace("\\040", " ")
                .replace("\\011", "\t")
                .replace("\\012", "\n")
                .replace("\\134", "\\");
            let mount = PathBuf::from(mount);
            path.starts_with(&mount).then(|| {
                (
                    mount.as_os_str().len(),
                    fields
                        .get(5)
                        .is_some_and(|options| options.split(',').any(|flag| flag == "noexec")),
                )
            })
        })
        .max_by_key(|(length, _)| *length)
        .is_some_and(|(_, noexec)| noexec)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn archive(kind: tar::EntryType) -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(kind);
        header.set_path("engine/worker.py").unwrap();
        header.set_size(0);
        header.set_mode(0o755);
        header.set_mtime(1);
        if kind.is_symlink() {
            header.set_link_name("/outside").unwrap();
        }
        header.set_cksum();
        builder.append(&header, std::io::empty()).unwrap();
        let data = builder.into_inner().unwrap();
        let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gzip.write_all(&data).unwrap();
        gzip.finish().unwrap()
    }

    #[test]
    fn extraction_keeps_executable_mode_without_restoring_mtime() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        unpack(
            archive(tar::EntryType::Regular).as_slice(),
            directory.path(),
        )
        .unwrap();
        let metadata = std::fs::metadata(directory.path().join("engine/worker.py")).unwrap();
        assert_eq!(metadata.permissions().mode() & 0o777, 0o755);
        assert_ne!(
            metadata.modified().unwrap(),
            std::time::UNIX_EPOCH + std::time::Duration::from_secs(1)
        );
    }

    #[test]
    fn extraction_error_names_entry_and_preserves_os_cause() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("engine"), "blocking file").unwrap();
        let error = unpack(
            archive(tar::EntryType::Regular).as_slice(),
            directory.path(),
        )
        .unwrap_err();
        assert!(error.message.contains("engine/worker.py"));
        assert!(error.message.contains("os error"));
        assert!(error.message.contains("TMPDIR"));
    }

    #[test]
    fn extraction_still_rejects_symlinks() {
        let directory = tempfile::tempdir().unwrap();
        assert!(
            unpack(
                archive(tar::EntryType::Symlink).as_slice(),
                directory.path()
            )
            .is_err()
        );
        assert!(!directory.path().join("engine/worker.py").exists());
    }

    #[test]
    fn mount_check_uses_deepest_matching_mount_and_decodes_paths() {
        let mounts = "1 0 0:1 / / rw - overlay overlay rw\n2 1 0:2 / /limited rw,noexec - tmpfs tmpfs rw\n3 2 0:3 / /limited/allowed rw - tmpfs tmpfs rw\n4 1 0:4 / /with\\040space rw,noexec - tmpfs tmpfs rw";
        assert!(mount_disallows_execution(Path::new("/limited/run"), mounts));
        assert!(!mount_disallows_execution(
            Path::new("/limited/allowed/run"),
            mounts
        ));
        assert!(!mount_disallows_execution(
            Path::new("/limited-other"),
            mounts
        ));
        assert!(mount_disallows_execution(
            Path::new("/with space/run"),
            mounts
        ));
    }
}
