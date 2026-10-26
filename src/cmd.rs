use std::{
    io::{self, BufRead, BufReader, Write},
    process::{Command, Stdio},
    thread::JoinHandle,
};

fn free_file_name(pattern: String, ext: String, numdigits: i32, num: i32) -> std::path::PathBuf {
    let to_name;
    if numdigits == 1 {
        to_name = format!("{}{:01}{}", pattern, num, ext);
    } else if numdigits == 2 {
        to_name = format!("{}{:02}{}", pattern, num, ext);
    } else if numdigits == 3 {
        to_name = format!("{}{:03}{}", pattern, num, ext);
    } else if numdigits == 4 {
        to_name = format!("{}{:04}{}", pattern, num, ext);
    } else if numdigits == 5 {
        to_name = format!("{}{:05}{}", pattern, num, ext);
    } else {
        to_name = format!("{}{:06}{}", pattern, num, ext);
    }
    let to = std::path::PathBuf::from(&to_name);
    to
}

pub fn rename_with_pattern(
    selected: Vec<std::path::PathBuf>,
    pattern: String,
    start_val: i32,
    numdigits: i32,
) {
    if !selected.is_empty() {
        let mut num = start_val;
        for path in selected {
            let mut ext = String::new();
            if let Some(extension) = path.extension() {
                ext = format!(
                    ".{}",
                    crate::parsers::osstr_to_string(extension.to_os_string())
                );
            }
            let name = crate::parsers::osstr_to_string(path.clone().into_os_string());
            if !path.is_dir() {
                let mut to = free_file_name(pattern.clone(), ext.clone(), numdigits, num);
                while to.is_file() {
                    num += 1;
                    to = free_file_name(pattern.clone(), ext.clone(), numdigits, num);
                }
                match std::fs::rename(path, to) {
                    Err(error) => log::error!("Failed to rename file {}: {}", name, error),
                    _ => {}
                }
                num += 1;
            }
        }
    }
}

// The code below works around the standard behaviour of printing and returning bytes.
// To parse the output of a program the Strings are much more useful.
// Therefore my version of capture returns a Vec<String>.
// Currently the source is no longer reproducible.

pub struct CmdRunner {
    pub cmd: Command,
}

fn tee<R, W>(reader: R, mut writer: W) -> JoinHandle<io::Result<Vec<String>>>
where
    R: BufRead + Send + 'static,
    W: Write + Send + 'static,
{
    std::thread::spawn(move || {
        let mut capture = Vec::new();

        for line in reader.lines() {
            let line = line?;

            capture.push(line.clone());
            writer.write_all(line.as_bytes())?;
        }

        Ok(capture)
    })
}

impl CmdRunner {
    pub fn new(cmd_str: &str) -> CmdRunner {
        let mut cmd = Command::new("script");

        cmd.arg("-qec").arg(cmd_str).arg("/dev/null");
        CmdRunner { cmd }
    }

    pub fn run_with_output(&mut self) -> io::Result<(Vec<String>, Vec<String>)> {
        self.cmd.stdin(Stdio::null());
        self.cmd.stdout(Stdio::piped());
        self.cmd.stderr(Stdio::piped());

        let mut child = self.cmd.spawn().expect("failed to spawn command");
        let stdout_pipe = child.stdout.take().unwrap();
        let stdout_thread = tee(BufReader::new(stdout_pipe), io::stdout());
        let stderr_pipe = child.stderr.take().unwrap();
        let stderr_thread = tee(BufReader::new(stderr_pipe), io::stderr());
        let stdout_output = stdout_thread.join().expect("failed to join stdout thread");
        let stderr_output = stderr_thread.join().expect("failed to join stderr thread");

        Ok((stdout_output?, stderr_output?))
    }
}
