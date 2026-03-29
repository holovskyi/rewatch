use process_wrap::std::*;
use std::collections::HashMap;
use std::io;
use std::process::{ExitStatus, Stdio};

pub struct ManagedChild {
    child: Box<dyn ChildWrapper>,
    last_status: Option<ExitStatus>,
    done: bool,
}

impl ManagedChild {
    pub fn spawn(args: &[String], env: &HashMap<String, String>) -> io::Result<Self> {
        if args.is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "empty command"));
        }

        let args = args.to_vec();
        let env = env.clone();
        let mut wrap = CommandWrap::with_new(&args[0], |cmd| {
            cmd.args(&args[1..]);
            cmd.envs(&env);
            cmd.stdin(Stdio::null());
        });

        #[cfg(unix)]
        wrap.wrap(ProcessGroup::leader());

        #[cfg(windows)]
        wrap.wrap(JobObject);

        let child = wrap.spawn()?;

        Ok(ManagedChild {
            child,
            last_status: None,
            done: false,
        })
    }

    pub fn kill_and_wait(&mut self) {
        if self.done {
            return;
        }
        if let Err(e) = self.child.start_kill() {
            eprintln!("rewatch: failed to kill process: {e}");
        }
        let _ = self.child.wait();
        self.done = true;
    }

    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        if self.done {
            return Ok(self.last_status);
        }
        let result = self.child.try_wait();
        if let Ok(Some(status)) = &result {
            self.last_status = Some(*status);
            self.done = true;
        }
        result
    }
}

impl Drop for ManagedChild {
    fn drop(&mut self) {
        self.kill_and_wait();
    }
}
