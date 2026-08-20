//! vCPU 主循环：KVM_RUN → exit 分发。
//! 与 Firecracker 的 vstate/vcpu.rs 同构，只是只支持 IO 和 HLT 两种 exit。

use crate::kvm::{KvmError, Vcpu, KVM_EXIT_HLT, KVM_EXIT_IO, KVM_EXIT_IO_OUT};
use crate::loader::COM1_PORT;

/// 跑到 guest hlt 为止。串口输出直接打到进程 stdout。
pub fn run_loop(vcpu: &Vcpu) -> Result<(), KvmError> {
    loop {
        vcpu.run()?;
        match vcpu.exit_reason() {
            KVM_EXIT_IO => {
                let (direction, port, count, data_offset) = vcpu.io_exit();
                if direction == KVM_EXIT_IO_OUT && port == COM1_PORT {
                    let data = vcpu.io_data(data_offset, count as usize);
                    // 串口是字节流，直接写 stdout
                    use std::io::Write;
                    std::io::stdout().write_all(data).unwrap();
                }
                // 其他端口的 IO 静默丢弃（v0.1 不 care）
            }
            KVM_EXIT_HLT => {
                // guest 执行了 hlt —— 我们的 guest 只在打印完后 hlt，干净退出
                return Ok(());
            }
            reason => return Err(KvmError::UnsupportedExit(reason)),
        }
    }
}
