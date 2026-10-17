//! Strict recycling: refuse an operation whose pre-delete notification says
//! recycling is unavailable, and require a recycle-bin item in PostDeleteItem.
use std::{
    os::windows::ffi::OsStrExt,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use windows::{
    Win32::{Foundation::E_ABORT, System::Com::*, UI::Shell::*},
    core::{HRESULT, PCWSTR, Ref, Result as WinResult, implement},
};

#[implement(IFileOperationProgressSink)]
struct RecycleGuard {
    recycled: Arc<AtomicBool>,
    blocked: Arc<AtomicBool>,
}

#[allow(non_snake_case, unused_variables)]
impl IFileOperationProgressSink_Impl for RecycleGuard_Impl {
    fn StartOperations(&self) -> WinResult<()> {
        Ok(())
    }
    fn FinishOperations(&self, hrresult: HRESULT) -> WinResult<()> {
        hrresult.ok()
    }
    fn PreRenameItem(
        &self,
        dwflags: u32,
        psiitem: Ref<IShellItem>,
        psznewname: &PCWSTR,
    ) -> WinResult<()> {
        Err(E_ABORT.into())
    }
    fn PostRenameItem(
        &self,
        dwflags: u32,
        psiitem: Ref<IShellItem>,
        psznewname: &PCWSTR,
        hrrename: HRESULT,
        psinewlycreated: Ref<IShellItem>,
    ) -> WinResult<()> {
        Ok(())
    }
    fn PreMoveItem(
        &self,
        dwflags: u32,
        psiitem: Ref<IShellItem>,
        psidestinationfolder: Ref<IShellItem>,
        psznewname: &PCWSTR,
    ) -> WinResult<()> {
        Err(E_ABORT.into())
    }
    fn PostMoveItem(
        &self,
        dwflags: u32,
        psiitem: Ref<IShellItem>,
        psidestinationfolder: Ref<IShellItem>,
        psznewname: &PCWSTR,
        hrmove: HRESULT,
        psinewlycreated: Ref<IShellItem>,
    ) -> WinResult<()> {
        Ok(())
    }
    fn PreCopyItem(
        &self,
        dwflags: u32,
        psiitem: Ref<IShellItem>,
        psidestinationfolder: Ref<IShellItem>,
        psznewname: &PCWSTR,
    ) -> WinResult<()> {
        Err(E_ABORT.into())
    }
    fn PostCopyItem(
        &self,
        dwflags: u32,
        psiitem: Ref<IShellItem>,
        psidestinationfolder: Ref<IShellItem>,
        psznewname: &PCWSTR,
        hrcopy: HRESULT,
        psinewlycreated: Ref<IShellItem>,
    ) -> WinResult<()> {
        Ok(())
    }
    fn PreDeleteItem(&self, dwflags: u32, psiitem: Ref<IShellItem>) -> WinResult<()> {
        if dwflags & TSF_DELETE_RECYCLE_IF_POSSIBLE.0 as u32 == 0 {
            self.blocked.store(true, Ordering::Relaxed);
            return Err(E_ABORT.into());
        }
        Ok(())
    }
    fn PostDeleteItem(
        &self,
        dwflags: u32,
        psiitem: Ref<IShellItem>,
        hrdelete: HRESULT,
        psinewlycreated: Ref<IShellItem>,
    ) -> WinResult<()> {
        hrdelete.ok()?;
        if psinewlycreated.as_ref().is_none() {
            return Err(E_ABORT.into());
        }
        self.recycled.store(true, Ordering::Relaxed);
        Ok(())
    }
    fn PreNewItem(
        &self,
        dwflags: u32,
        psidestinationfolder: Ref<IShellItem>,
        psznewname: &PCWSTR,
    ) -> WinResult<()> {
        Err(E_ABORT.into())
    }
    fn PostNewItem(
        &self,
        dwflags: u32,
        psidestinationfolder: Ref<IShellItem>,
        psznewname: &PCWSTR,
        psztemplatename: &PCWSTR,
        dwfileattributes: u32,
        hrnew: HRESULT,
        psinewitem: Ref<IShellItem>,
    ) -> WinResult<()> {
        Ok(())
    }
    fn UpdateProgress(&self, iworktotal: u32, iworksofar: u32) -> WinResult<()> {
        Ok(())
    }
    fn ResetTimer(&self) -> WinResult<()> {
        Ok(())
    }
    fn PauseTimer(&self) -> WinResult<()> {
        Ok(())
    }
    fn ResumeTimer(&self) -> WinResult<()> {
        Ok(())
    }
}

struct Attempt {
    result: WinResult<()>,
    recycled: bool,
    blocked: bool,
}

fn attempt(path: &Path) -> Attempt {
    let recycled = Arc::new(AtomicBool::new(false));
    let blocked = Arc::new(AtomicBool::new(false));
    let result = (|| -> WinResult<()> {
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
            struct ComGuard;
            impl Drop for ComGuard {
                fn drop(&mut self) {
                    unsafe {
                        CoUninitialize();
                    }
                }
            }
            let _guard = ComGuard;
            let operation: IFileOperation =
                CoCreateInstance(&FileOperation, None, CLSCTX_INPROC_SERVER)?;
            operation.SetOperationFlags(
                FOF_NO_UI
                    | FOF_ALLOWUNDO
                    | FOF_WANTNUKEWARNING
                    | FOFX_RECYCLEONDELETE
                    | FOFX_EARLYFAILURE,
            )?;
            let raw: Vec<u16> = path.as_os_str().encode_wide().collect();
            let prefix: Vec<u16> = r"\\?\".encode_utf16().collect();
            let unc: Vec<u16> = r"\\?\UNC\".encode_utf16().collect();
            let mut shell_path = if raw.starts_with(&unc) {
                let mut p = vec![92, 92];
                p.extend_from_slice(&raw[8..]);
                p
            } else if raw.starts_with(&prefix) {
                raw[4..].to_vec()
            } else {
                raw
            };
            shell_path.push(0);
            let item: IShellItem = SHCreateItemFromParsingName(PCWSTR(shell_path.as_ptr()), None)?;
            let sink: IFileOperationProgressSink = RecycleGuard {
                recycled: recycled.clone(),
                blocked: blocked.clone(),
            }
            .into();
            operation.DeleteItem(&item, &sink)?;
            operation.PerformOperations()?;
            if operation.GetAnyOperationsAborted()?.as_bool() || !recycled.load(Ordering::Relaxed) {
                return Err(E_ABORT.into());
            }
            Ok(())
        }
    })();
    Attempt {
        result,
        recycled: recycled.load(Ordering::Relaxed),
        blocked: blocked.load(Ordering::Relaxed),
    }
}

fn sharing_conflict(code: u32) -> bool {
    matches!(code, 0x80270027 | 0x80270028 | 0x80070020 | 0x80070021)
}

fn describe_failure(code: u32, blocked: bool, partial: bool) -> String {
    let reason = if blocked {
        "该目录无法安全回收，已拒绝执行永久删除"
    } else {
        match code {
            0x80270028 => {
                "Windows 报告回收目标存在共享冲突，相关文件可能被占用。请关闭使用该目录的程序后重试"
            }
            0x80270027 | 0x80070020 | 0x80070021 => {
                "目录或其中的文件被其他程序占用。请关闭相关开发工具、运行中的程序或文件窗口后重试"
            }
            0x80270021 | 0x80270022 | 0x80070005 => {
                "目录或回收站访问被拒绝，请检查对应目录的访问权限后重试"
            }
            0x80270037 | 0x8027000D => {
                "目录或文件超过 Windows 当前可回收的大小限制，请检查该磁盘的回收站设置"
            }
            0x80270038 | 0x8027001D | 0x8027001E => {
                "目录路径过长，Windows 无法完成回收。请打开目录检查路径"
            }
            0x8027003A => "该磁盘的回收站不可用，请检查磁盘连接和回收站状态",
            0x80270036 => "Windows 要求永久删除该项目，已拒绝继续",
            0x80270023 | 0x80270024 | 0x80070002 | 0x80070003 => {
                "目录或回收目标已不存在，请重新扫描后再试"
            }
            0x80004004 | 0x800704C7 => "回收操作已取消或未完成，请重新扫描后再试",
            _ => "Windows 回收操作失败，请检查目录占用、访问权限及回收站状态后重试",
        }
    };
    let extra = if partial {
        "操作可能已部分完成，请重新扫描确认。"
    } else {
        ""
    };
    format!("{reason}（0x{code:08X}）。{extra}未改用永久删除。")
}

fn can_retry(outcome: &Attempt) -> bool {
    !outcome.blocked
        && !outcome.recycled
        && outcome
            .result
            .as_ref()
            .is_err_and(|e| sharing_conflict(e.code().0 as u32))
}

pub(crate) fn recycle(path: &Path) -> Result<(), String> {
    let path = path.to_owned();
    // Retry only once, on a sharing violation, on the same ordinary filesystem item.
    // Each attempt retains the pre-delete recycling guard and never falls back to deletion.
    std::thread::spawn(move || {
        let original = std::fs::symlink_metadata(&path).ok();
        let mut outcome = attempt(&path);
        if can_retry(&outcome) {
            std::thread::sleep(std::time::Duration::from_millis(75));
            let unchanged = original.as_ref().is_some_and(|before| {
                before.created().is_ok()
                    && std::fs::symlink_metadata(&path).is_ok_and(|now| {
                        !crate::is_link(&now)
                            && now.file_type() == before.file_type()
                            && now.created().ok() == before.created().ok()
                    })
            });
            if unchanged {
                outcome = attempt(&path);
            }
        }
        outcome
            .result
            .map_err(|e| describe_failure(e.code().0 as u32, outcome.blocked, outcome.recycled))
    })
    .join()
    .map_err(|_| "回收站线程异常，操作未确认完成，请重新扫描".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn denies_permanent_delete_before_operation() {
        let sink: IFileOperationProgressSink = RecycleGuard {
            recycled: Arc::new(AtomicBool::new(false)),
            blocked: Arc::new(AtomicBool::new(false)),
        }
        .into();
        unsafe {
            assert!(sink.PreDeleteItem(0, None).is_err());
            assert!(
                sink.PreDeleteItem(TSF_DELETE_RECYCLE_IF_POSSIBLE.0 as u32, None)
                    .is_ok()
            );
        }
    }

    #[test]
    fn sharing_errors_are_actionable_and_retry_never_bypasses_recycling_guard() {
        let code = COPYENGINE_E_SHARING_VIOLATION_DEST.0 as u32;
        let message = describe_failure(code, false, false);
        assert!(message.contains("回收目标存在共享冲突"));
        assert!(message.contains("0x80270028"));
        assert!(message.contains("未改用永久删除"));
        let mut outcome = Attempt {
            result: Err(windows::core::Error::from(HRESULT(code as i32))),
            recycled: false,
            blocked: false,
        };
        assert!(can_retry(&outcome));
        outcome.blocked = true;
        assert!(!can_retry(&outcome));
        outcome.blocked = false;
        outcome.recycled = true;
        assert!(!can_retry(&outcome));
        outcome.recycled = false;
        outcome.result = Err(windows::Win32::Foundation::E_ACCESSDENIED.into());
        assert!(!can_retry(&outcome));
        assert!(describe_failure(0x80270037, false, false).contains("大小限制"));
    }

    #[test]
    #[ignore = "explicit opt-in: verifies a new locked fixture is preserved"]
    fn recycling_preserves_generated_locked_fixture() {
        use std::os::windows::fs::OpenOptionsExt;
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("huanj-lock-verification.txt");
        std::fs::write(&file, "Generated lock verification").unwrap();
        let path = std::fs::canonicalize(&file).unwrap();
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&file)
            .unwrap();
        let result = recycle(&path);
        drop(lock);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("占用"));
        assert_eq!(
            std::fs::read_to_string(file).unwrap(),
            "Generated lock verification"
        );
    }

    #[test]
    #[ignore = "explicit opt-in: moves only a new nonempty fixture directory into the recycle bin"]
    fn recycles_generated_directory_fixture() {
        let temp = tempfile::tempdir().unwrap();
        let directory = temp.path().join("目录,测试");
        std::fs::create_dir_all(directory.join("nested")).unwrap();
        std::fs::write(
            directory.join("nested/fixture.txt"),
            "Generated directory verification",
        )
        .unwrap();
        let path = std::fs::canonicalize(&directory).unwrap();
        assert!(path.starts_with(std::fs::canonicalize(temp.path()).unwrap()));
        recycle(&path).unwrap();
        assert!(!directory.exists());
    }

    #[test]
    #[ignore = "explicit opt-in: moves only a newly generated fixture into the real recycle bin"]
    fn recycles_only_generated_fixture() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("huanj-recycle-verification.txt");
        std::fs::write(&file, "Generated solely to verify huanj recycling.").unwrap();
        let canonical = std::fs::canonicalize(&file).unwrap();
        assert!(canonical.starts_with(std::fs::canonicalize(temp.path()).unwrap()));
        recycle(&canonical).unwrap();
        assert!(!file.exists());
    }
}
