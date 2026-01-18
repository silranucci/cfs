use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, fs};

const STACK_SIZE: usize = 1024 * 1024; // 1MB stack

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        help(&args[0]);
    } else {
        match args[1].as_str() {
            "run" => {
                if args.len() < 3 {
                    eprintln!("Need a command to run");
                    std::process::exit(1);
                }
                run(&args);
            }
            _ => {
                eprintln!("Unknown command {}", &args[1]);
            }
        }
    }
}

fn help(exec_name: &String) {
    println!("Usage: {} run <command> [args...]", exec_name);
    println!("Example: {} run /bin/bash", exec_name);
}

fn run(args: &[String]) {
    println!("Running {:?} as PID {}", &args[2..], std::process::id());

    let mut stack = vec![0u8; STACK_SIZE];
    let stack_top = stack.as_mut_ptr().wrapping_add(STACK_SIZE); // stack grows down

    let flags = libc::CLONE_NEWUTS | libc::CLONE_NEWPID | libc::SIGCHLD | libc::CLONE_NEWNS;

    let child_args: Vec<String> = args.iter().skip(2).cloned().collect();
    let pid = unsafe {
        libc::clone(
            child_func,
            stack_top as *mut libc::c_void,
            flags,
            &child_args as *const Vec<String> as *mut libc::c_void,
        )
    };
    if pid < 0 {
        eprintln!("clone failed: {}", std::io::Error::last_os_error());
        std::process::exit(1);
    }

    unsafe {
        if libc::unshare(libc::CLONE_NEWNS) != 0 {
            panic!("unshare failed: {}", std::io::Error::last_os_error());
        }
    }

    let mut status: i32 = 0;
    unsafe {
        libc::waitpid(pid, &mut status, 0);
    }
}

extern "C" fn child_func(arg: *mut libc::c_void) -> i32 {
    let path = Path::new("/home/ubuntu-fs");
    let args = unsafe { &*(arg as *const Vec<String>) };

    ensure_debootstrap();
    bootstrap_rootfs(&path);
    set_hostname("container");
    chroot(&path);
    mount_proc();
    cg();

    println!("Child running as PID {}", std::process::id());
    let status = run_cmd(&args);
    unmount_proc();
    return status;
}

fn chroot(path: &Path) {
    unsafe {
        if libc::chroot(CString::new(path.to_str().unwrap()).unwrap().as_ptr()) != 0 {
            panic!("chroot failed");
        }
    }
    std::env::set_current_dir("/").expect("chdir failed");
}

fn run_cmd(args: &[String]) -> i32 {
    let mut cmd = Command::new(&args[0]);
    if args.len() > 1 {
        cmd.args(&args[1..]);
    }

    let status = cmd.status().expect("failed to run command");
    status.code().unwrap_or(1)
}

fn mount_proc() {
    let source = CString::new("proc").unwrap();
    let target = CString::new("/proc").unwrap();
    let fstype = CString::new("proc").unwrap();

    unsafe {
        if libc::mount(
            source.as_ptr(),
            target.as_ptr(),
            fstype.as_ptr(),
            0,
            std::ptr::null(),
        ) != 0
        {
            panic!("mount failed: {}", std::io::Error::last_os_error());
        }
    }
}

fn unmount_proc() {
    let target = CString::new("/proc").unwrap();

    unsafe {
        if libc::umount(target.as_ptr()) != 0 {
            panic!("unmount failed: {}", std::io::Error::last_os_error());
        }
    }
}

fn set_hostname(name: &str) {
    let ret = unsafe {
        libc::sethostname(
            name.as_ptr() as *const libc::c_char,
            name.len().try_into().unwrap(),
        )
    };
    if ret != 0 {
        eprintln!(
            "Failed to set hostname: {}",
            std::io::Error::last_os_error()
        );
    }
}

fn cg() {
    let cgroups = PathBuf::from("/sys/fs/cgroup/");
    let pids = cgroups.join("pids");

    fs::create_dir_all(&pids).expect("Failed to create cgroup dir");
    fs::write(pids.join("pids.max"), "20").expect("Failed to write pids.max");
    fs::write(pids.join("notify_on_release"), "1").expect("Failed to write notify_on_release");
    fs::write(pids.join("cgroup.procs"), std::process::id().to_string())
        .expect("Failed to write cgroup.procs");
}

fn bootstrap_rootfs(path: &Path) {
    if path.exists() {
        return;
    }

    let mirror = if cfg!(target_arch = "aarch64") {
        "http://ports.ubuntu.com/ubuntu-ports"
    } else {
        "http://archive.ubuntu.com/ubuntu"
    };

    let status = Command::new("debootstrap")
        .args(["--variant=minbase", "jammy", path.to_str().unwrap(), mirror])
        .status()
        .expect("failed to run debootstrap");

    if !status.success() {
        panic!("debootstrap failed");
    }
}

fn ensure_debootstrap() {
    if Command::new("which")
        .arg("debootstrap")
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
    {
        return;
    }

    println!("Installing debootstrap...");
    Command::new("apt-get").args(["update"]).status().ok();
    Command::new("apt-get")
        .args(["install", "-y", "debootstrap"])
        .status()
        .expect("failed to install debootstrap");
}

