//! Launch profiles share the existing PTY and never concatenate shell commands.
use crate::model::Launch;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct Profile { pub title: String, pub program: String, pub distribution: Option<String> }
impl Profile {
    pub fn launch(&self, cwd: &Path) -> Launch {
        let args = match &self.distribution {
            Some(name) => vec!["--distribution".into(), name.clone(), "--cd".into(), cwd.to_string_lossy().into_owned()],
            None => vec![],
        };
        Launch { program: self.program.clone(), args }
    }
}
#[derive(Debug, Clone, Default)]
pub struct Profiles { pub items: Vec<Profile>, pub warning: Option<String> }

#[cfg(windows)]
fn decode(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xff,0xfe]) || bytes.iter().skip(1).step_by(2).any(|b| *b == 0) {
        String::from_utf16_lossy(&bytes.chunks_exact(2).map(|b| u16::from_le_bytes([b[0],b[1]])).collect::<Vec<_>>()).trim_start_matches('\u{feff}').to_owned()
    } else { String::from_utf8_lossy(bytes).into_owned() }
}
#[cfg(windows)]
pub fn discover() -> Profiles {
    use std::{os::windows::process::CommandExt, process::Command};
    let mut profiles = Profiles::default();
    let system = std::path::PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into())).join("System32");
    for (title,path) in [("Windows PowerShell",system.join("WindowsPowerShell/v1.0/powershell.exe")),("命令提示符",system.join("cmd.exe"))] {
        if path.is_file() { profiles.items.push(Profile { title:title.into(), program:path.to_string_lossy().into_owned(), distribution:None }); }
    }
    let pwsh = std::env::var_os("PATH").and_then(|paths|std::env::split_paths(&paths).map(|p|p.join("pwsh.exe")).find(|p|p.is_file()))
        .or_else(||std::env::var_os("ProgramFiles").map(|p|std::path::PathBuf::from(p).join("PowerShell/7/pwsh.exe")).filter(|p|p.is_file()));
    if let Some(path) = pwsh { profiles.items.push(Profile {title:"PowerShell 7".into(),program:path.to_string_lossy().into_owned(),distribution:None}); }
    let wsl = system.join("wsl.exe");
    if !wsl.is_file() { return profiles; }
    match Command::new(&wsl).args(["--list","--quiet"]).creation_flags(0x08000000).output() {
        Ok(output) if output.status.success() => {
            for name in decode(&output.stdout).lines().map(str::trim).filter(|n|!n.is_empty() && !n.chars().any(char::is_control)) {
                profiles.items.push(Profile { title:format!("WSL · {name}"),program:wsl.to_string_lossy().into_owned(),distribution:Some(name.into()) });
            }
        }
        Ok(_) => profiles.warning=Some("未能读取 WSL 发行版；请确认 WSL 已安装并可用。Windows Shell 仍可使用。".into()),
        Err(error) => profiles.warning=Some(format!("读取 WSL 发行版失败：{error}")),
    }
    profiles
}
#[cfg(not(windows))]
pub fn discover() -> Profiles { Profiles {items:vec![Profile {title:"系统 Shell".into(),program:std::env::var("SHELL").unwrap_or_else(|_|"/bin/sh".into()),distribution:None}],warning:None} }

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn wsl_path_and_distribution_remain_separate_arguments() {
        let p=Profile {title:"WSL".into(),program:"wsl.exe".into(),distribution:Some("Ubuntu-24.04".into())};
        assert_eq!(p.launch(Path::new("D:\\项目 文件\\a&b")).args,vec!["--distribution","Ubuntu-24.04","--cd","D:\\项目 文件\\a&b"]);
    }
    #[cfg(windows)] #[test] fn decodes_wsl_utf16_and_utf8_lists() {
        let s="Ubuntu-24.04\r\nCentOS-7\r\n";
        let bytes:Vec<_>=s.encode_utf16().flat_map(u16::to_le_bytes).collect();
        assert_eq!(decode(&bytes),s);assert_eq!(decode(s.as_bytes()),s);
    }
}
