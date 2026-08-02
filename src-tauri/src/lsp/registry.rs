use std::path::PathBuf;

pub struct ServerSpec {
    pub command: &'static str,
    pub args: &'static [&'static str],
}

pub fn server_for_language(language_id: &str) -> Option<ServerSpec> {
    match language_id {
        "rust" => Some(ServerSpec {
            command: "rust-analyzer",
            args: &[],
        }),
        "typescript" | "javascript" | "typescriptreact" | "javascriptreact" => {
            Some(ServerSpec {
                command: "typescript-language-server",
                args: &["--stdio"],
            })
        }
        "python" => Some(ServerSpec {
            command: "pyright-langserver",
            args: &["--stdio"],
        }),
        "go" => Some(ServerSpec {
            command: "gopls",
            args: &[],
        }),
        "c" | "cpp" => Some(ServerSpec {
            command: "clangd",
            args: &[],
        }),
        _ => None,
    }
}

pub fn resolve_on_path(command: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(command);
        if is_executable(&candidate) {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            let exe = dir.join(format!("{}.exe", command));
            if is_executable(&exe) {
                return Some(exe);
            }
        }
    }
    None
}

#[cfg(unix)]
fn is_executable(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.is_file()
        && std::fs::metadata(path)
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
}

#[cfg(windows)]
fn is_executable(path: &std::path::Path) -> bool {
    path.is_file()
}

pub fn path_to_uri(path: &str) -> String {
    let mut out = String::from("file://");
    if !path.starts_with('/') {
        out.push('/');
    }
    for byte in path.bytes() {
        match byte {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'.'
            | b'_'
            | b'~'
            | b'/' => out.push(byte as char),
            _ => out.push_str(&format!("%{:02X}", byte)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_encodes_special_chars() {
        assert_eq!(
            path_to_uri("/Users/me/my project/src"),
            "file:///Users/me/my%20project/src"
        );
        assert_eq!(path_to_uri("/a/b.rs"), "file:///a/b.rs");
    }
}
