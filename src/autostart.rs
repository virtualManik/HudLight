//! Start with Windows: a value under the current user's `Run` key.

#[cfg(windows)]
pub fn set(enabled: bool) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
    const NAME: &str = "HudLight";
    let mut cmd = std::process::Command::new("reg");
    if enabled {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let value = format!("\"{}\" --minimized", exe.display());
        cmd.args(["add", KEY, "/v", NAME, "/t", "REG_SZ", "/d", &value, "/f"]);
    } else {
        cmd.args(["delete", KEY, "/v", NAME, "/f"]);
    }
    let out = cmd.creation_flags(CREATE_NO_WINDOW).output().map_err(|e| e.to_string())?;
    // Deleting a value that isn't there is fine.
    if out.status.success() || !enabled {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
    }
}

#[cfg(not(windows))]
pub fn set(_enabled: bool) -> Result<(), String> {
    Err("Start at login is only wired up on Windows.".into())
}
