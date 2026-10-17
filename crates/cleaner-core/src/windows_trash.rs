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

pub(crate) fn recycle(path: &Path) -> Result<(), String> {
    let path = path.to_owned();
    // An isolated STA avoids inheriting an incompatible COM apartment from a pool.
    std::thread::spawn(move || -> WinResult<()> {
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
            let recycled = Arc::new(AtomicBool::new(false));
            let sink: IFileOperationProgressSink = RecycleGuard {
                recycled: recycled.clone(),
            }
            .into();
            operation.DeleteItem(&item, &sink)?;
            operation.PerformOperations()?;
            if operation.GetAnyOperationsAborted()?.as_bool() || !recycled.load(Ordering::Relaxed) {
                return Err(E_ABORT.into());
            }
            Ok(())
        }
    })
    .join()
    .map_err(|_| "回收站线程异常".to_string())?
    .map_err(|e| format!("无法安全移入回收站，已拒绝永久删除：{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn denies_permanent_delete_before_operation() {
        let sink: IFileOperationProgressSink = RecycleGuard {
            recycled: Arc::new(AtomicBool::new(false)),
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
