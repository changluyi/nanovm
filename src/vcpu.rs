//! vCPU 主循环：KVM_RUN → exit 分发。
//! 与 Firecracker 的 vstate/vcpu.rs 同构，只是只支持 IO 和 HLT 两种 exit。

use crate::kvm::{KvmError, Vcpu, KVM_EXIT_HLT, KVM_EXIT_IO, KVM_EXIT_IO_IN, KVM_EXIT_IO_OUT};
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
                if port != COM1_PORT {
                    continue; // 其他端口的 IO 静默丢弃（不 care）
                }
                if direction == KVM_EXIT_IO_IN {
                    // 设备虚拟化的另一半：VMM 替 guest 读串口。
                    // 此处阻塞在 stdin 上 —— guest 被 KVM_RUN 挂起，等宿主机喂数据。
                    // host EOF 翻译成 EOT(0x04)，guest 收到后 hlt，管道场景也能干净退出。
                    let data = vcpu.io_data_mut(data_offset, count as usize);
                    let n = std::io::Read::read(&mut std::io::stdin(), data).unwrap_or(0);
                    data[n..].fill(0x04);
                }
                if debug {
                    let dir = if direction == KVM_EXIT_IO_OUT { "OUT" } else { "IN " };
                    let data = vcpu.io_data(data_offset, count as usize);
                    eprintln!(
                        "[exit #{:<3}] IO   port={:#06x} {} count={} data={:?}",
                        stats.total_exits, port, dir, count,
                        String::from_utf8_lossy(data)
                    );
                }
                if direction == KVM_EXIT_IO_OUT {
                    let data = vcpu.io_data(data_offset, count as usize);
                    use std::io::Write;
                    let mut out = std::io::stdout();
                    out.write_all(data).unwrap();
                    out.flush().unwrap(); // 交互模式下 guest 在等下一字节，必须立刻可见
                }
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
