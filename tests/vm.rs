//! 集成测试：真正把 VM 跑起来验证行为。
//! 前提：环境有 /dev/kvm（没有则跳过，CI 上用裸金属 runner）。

use std::process::Command;

fn have_kvm() -> bool {
    std::path::Path::new("/dev/kvm").exists()
}

fn run_nanovm(envs: &[(&str, &str)]) -> (String, String, Option<i32>) {
    let bin = env!("CARGO_BIN_EXE_nanovm");
    let mut cmd = Command::new(bin);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("启动 nanovm 失败");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code(),
    )
}

#[test]
fn hello_world() {
    if !have_kvm() {
        eprintln!("跳过：无 /dev/kvm");
        return;
    }
    let (stdout, _stderr, code) = run_nanovm(&[]);
    assert_eq!(code, Some(0), "应干净退出");
    assert!(
        stdout.contains("hello from nanovm (64-bit)"),
        "stdout 应包含 guest 输出，实际: {stdout}"
    );
}

#[test]
fn debug_mode_reports_exits() {
    if !have_kvm() {
        eprintln!("跳过：无 /dev/kvm");
        return;
    }
    let (_stdout, stderr, code) = run_nanovm(&[("NANOVM_DEBUG", "1")]);
    assert_eq!(code, Some(0));
    // 每个字符一次 IO exit + 一次 HLT
    assert!(stderr.contains("IO   port=0x03f8"), "应打印 IO exit 细节");
    assert!(stderr.contains("HLT"), "应以 HLT 结束");
    assert!(
        stderr.contains("VM exit"),
        "应打印 exit 统计，实际: {stderr}"
    );
}
