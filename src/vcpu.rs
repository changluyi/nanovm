//! vCPU 主循环：KVM_RUN → exit 分发。
//! 与 Firecracker 的 vstate/vcpu.rs 同构，只是只支持 IO 和 HLT 两种 exit。

use crate::kvm::{KvmError, Vcpu, KVM_EXIT_HLT, KVM_EXIT_IO, KVM_EXIT_IO_OUT};
use crate::loader::COM1_PORT;

/// NANOVM_DEBUG=1 时打印每次 exit 的细节
fn debug_enabled() -> bool {
    std::env::var("NANOVM_DEBUG").is_ok()
}

pub struct RunStats {
    pub total_exits: u64,
    pub io_exits: u64,
}

/// 跑到 guest hlt 为止。串口输出直接打到进程 stdout。
pub fn run_loop(vcpu: &Vcpu) -> Result<RunStats, KvmError> {
    let debug = debug_enabled();
    let mut stats = RunStats { total_exits: 0, io_exits: 0 };

    loop {
        vcpu.run()?;
        stats.total_exits += 1;

        match vcpu.exit_reason() {
            KVM_EXIT_IO => {
                let (direction, port, count, data_offset) = vcpu.io_exit();
                stats.io_exits += 1;
                if debug {
                    let dir = if direction == KVM_EXIT_IO_OUT { "OUT" } else { "IN " };
                    let data = vcpu.io_data(data_offset, count as usize);
                    eprintln!(
                        "[exit #{:<3}] IO   port={:#06x} {} count={} data={:?}",
                        stats.total_exits, port, dir, count,
                        String::from_utf8_lossy(data)
                    );
                }
                if direction == KVM_EXIT_IO_OUT && port == COM1_PORT {
                    let data = vcpu.io_data(data_offset, count as usize);
                    use std::io::Write;
                    std::io::stdout().write_all(data).unwrap();
                }
                // 其他端口的 IO 静默丢弃（v0.1 不 care）
            }
            KVM_EXIT_HLT => {
                if debug {
                    let r = vcpu.get_regs().ok();
                    eprintln!("[exit #{:<3}] HLT  rip={:#010x} rflags={:#010x} rax={:#06x}",
                        stats.total_exits,
                        r.as_ref().map(|r| r.rip).unwrap_or(0),
                        r.as_ref().map(|r| r.rflags).unwrap_or(0),
                        r.as_ref().map(|r| r.rax).unwrap_or(0),
                    );
                    if let Some(r) = r {
                        eprintln!("              完整寄存器: {r:?}");
                    }
                }
                // guest 执行了 hlt —— 我们的 guest 只在打印完后 hlt，干净退出
                return Ok(stats);
            }
            reason => return Err(KvmError::UnsupportedExit(reason)),
        }
    }
}
