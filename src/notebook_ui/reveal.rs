//! Windows shell integration, isolated from notebook state and headless tests.
use std::{
    ffi::OsString,
    io,
    path::{Component, Path, Prefix},
    process::Command,
};

fn explorer_path(path: &Path) -> io::Result<OsString> {
    let mut components = path.components();
    let Some(Component::Prefix(prefix)) = components.next() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Reveal requires an absolute path.",
        ));
    };
    let mut result = match prefix.kind() {
        Prefix::VerbatimDisk(drive) => OsString::from(format!("{}:", char::from(drive))),
        Prefix::VerbatimUNC(server, share) => {
            let mut result = OsString::from(r"\\");
            result.push(server);
            result.push(r"\");
            result.push(share);
            result
        }
        Prefix::Disk(_) | Prefix::UNC(_, _) if path.is_absolute() => {
            return Ok(path.as_os_str().to_owned());
        }
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Unsupported Explorer path.",
            ));
        }
    };
    // Preserve native strings, including names that cannot be represented as UTF-8.
    for component in components {
        match component {
            Component::RootDir => result.push(r"\"),
            Component::Normal(name) => {
                if !result.as_encoded_bytes().ends_with(b"\\") {
                    result.push(r"\");
                }
                result.push(name);
            }
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Unsupported Explorer path.",
                ));
            }
        }
    }
    Ok(result)
}

fn command(path: &Path) -> io::Result<Command> {
    let mut command = Command::new("explorer.exe");
    command.arg("/select,").arg(explorer_path(path)?);
    Ok(command)
}

#[cfg(not(test))]
pub(super) fn launch(path: &Path) -> io::Result<()> {
    command(path)?.spawn().map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};

    #[test]
    fn explorer_arguments_preserve_paths_without_a_shell() {
        for (source, expected) in [
            (r"C:\notes\hello.md", r"C:\notes\hello.md"),
            (
                r"\\?\C:\my notes\café 你好, &.md",
                r"C:\my notes\café 你好, &.md",
            ),
            (
                r"\\?\UNC\server\share\my notes\a.md",
                r"\\server\share\my notes\a.md",
            ),
            (r"\\server\share\folder", r"\\server\share\folder"),
        ] {
            let command = command(Path::new(source)).unwrap();
            assert_eq!(command.get_program(), "explorer.exe");
            assert_eq!(
                command.get_args().collect::<Vec<_>>(),
                [
                    std::ffi::OsStr::new("/select,"),
                    std::ffi::OsStr::new(expected)
                ]
            );
        }
        assert!(command(Path::new("relative.md")).is_err());
        assert!(command(Path::new(r"\\?\GLOBALROOT\Device\HarddiskVolume1")).is_err());
        let mut units = r"\\?\C:\notes\".encode_utf16().collect::<Vec<_>>();
        units.push(0xD800);
        let path = OsString::from_wide(&units);
        assert_eq!(
            explorer_path(Path::new(&path))
                .unwrap()
                .encode_wide()
                .collect::<Vec<_>>(),
            units[4..]
        );
    }
}
